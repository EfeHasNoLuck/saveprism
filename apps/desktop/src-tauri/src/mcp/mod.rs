//! The MCP server of SavePrism: the desktop program itself speaks the Model Context Protocol over
//! stdio, so an AI tool can read and edit the save the user has open.
//!
//! The transport is newline-delimited JSON-RPC 2.0: one JSON object per line in, one per line out,
//! nothing else on stdout (diagnostics go to stderr only). The server holds one open document,
//! exactly like the window does, and every tool calls the existing [`crate::session::Session`]
//! methods; no editing logic lives here. [`run_stdio`] wires the process's stdin and stdout,
//! [`serve`] is the same loop over any reader and writer so it can be tested in process.

mod tools;

#[cfg(test)]
mod tests;

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::session::{ApiError, Session, app_codes};

/// Protocol versions this server knows, newest first.
const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The instructions shown to the client after `initialize`.
const INSTRUCTIONS: &str = "Open a save with open_save; read it with game_view (named RPG Maker sections), list_values and search_values. Change values with propose_edits, which changes memory only, then show pending_changes to the user and call save_changes only after the user agreed. Every save makes a backup first and is verified. Paths are the JSON Pointers the read tools return.";

/// The one open document and the backup folder (tests pass their own).
pub(super) struct ServerState {
    /// The open document, if any.
    pub(super) session: Option<Session>,
    /// The backup folder passed to [`Session::open`]; `None` uses the app's default.
    pub(super) backup_root: Option<PathBuf>,
}

/// The message of a tool result when handling one line panicked.
const PANIC_MESSAGE: &str =
    "SavePrism hit an internal error; the save was closed without writing. Open it again.";

/// A JSON-RPC error (as opposed to a tool result that carries an [`ApiError`]).
pub(super) struct RpcError {
    /// JSON-RPC error code.
    pub(super) code: i64,
    /// English message.
    pub(super) message: String,
}

impl RpcError {
    /// An invalid-arguments error (`-32602`).
    pub(super) fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    /// The JSON object of this error for the response.
    fn value(&self) -> Value {
        json!({"code": self.code, "message": self.message})
    }
}

/// What a tool call can fail with: a JSON-RPC error or a tool result marked `isError`.
pub(super) enum CallError {
    /// The request itself is wrong (unknown tool, arguments of the wrong shape).
    Protocol(RpcError),
    /// The tool ran and failed with an [`ApiError`] (the result carries it).
    Tool(ApiError),
}

impl From<ApiError> for CallError {
    fn from(error: ApiError) -> Self {
        Self::Tool(error)
    }
}

impl From<RpcError> for CallError {
    fn from(error: RpcError) -> Self {
        Self::Protocol(error)
    }
}

/// Runs the MCP server on the process's stdin and stdout and returns the exit code.
pub fn run_stdio() -> i32 {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve(stdin.lock(), &mut stdout.lock(), None) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("SP-APP-001: the MCP server stopped: {error}");
            1
        }
    }
}

/// Serves newline-delimited JSON-RPC 2.0 until `input` ends. Every reply is one compact JSON object
/// on its own line, flushed at once; notifications get no reply. A line that is not UTF-8 gets a
/// parse error and the loop goes on.
pub fn serve(
    mut input: impl BufRead,
    output: &mut impl Write,
    backup_root: Option<PathBuf>,
) -> std::io::Result<()> {
    let mut state = ServerState {
        session: None,
        backup_root,
    };
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        if input.read_until(b'\n', &mut buffer)? == 0 {
            break;
        }
        if buffer.last() == Some(&b'\n') {
            buffer.pop();
        }
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        let line = match std::str::from_utf8(&buffer) {
            Ok(line) => line,
            Err(_) => {
                write_reply(
                    output,
                    &error_response(
                        Value::Null,
                        RpcError {
                            code: -32700,
                            message: "parse error: the line is not valid UTF-8".to_string(),
                        },
                    ),
                )?;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_line(&mut state, line) {
            write_reply(output, &reply)?;
        }
    }
    Ok(())
}

/// Writes one reply as a compact JSON object on its own line and flushes it.
fn write_reply(output: &mut impl Write, reply: &Value) -> std::io::Result<()> {
    let text = serde_json::to_string(reply).map_err(std::io::Error::other)?;
    output.write_all(text.as_bytes())?;
    output.write_all(b"\n")?;
    output.flush()
}

/// Handles one input line. `None` means no reply (a notification).
fn handle_line(state: &mut ServerState, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(_) => {
            return Some(error_response(
                Value::Null,
                RpcError {
                    code: -32700,
                    message: "parse error: the line is not valid JSON".to_string(),
                },
            ));
        }
    };
    if message.is_array() {
        // Batches are not part of the stdio transport this server speaks.
        return Some(error_response(
            Value::Null,
            RpcError {
                code: -32600,
                message: "invalid request: a batch is not supported".to_string(),
            },
        ));
    }
    let Some(object) = message.as_object() else {
        return Some(error_response(
            Value::Null,
            RpcError {
                code: -32600,
                message: "invalid request: the message is not an object".to_string(),
            },
        ));
    };
    let notification = !object.contains_key("id");
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        if notification {
            return None;
        }
        return Some(error_response(
            id,
            RpcError {
                code: -32600,
                message: "invalid request: no method".to_string(),
            },
        ));
    };
    if notification {
        // The only notifications a client sends here need no work (initialized, cancelled).
        return None;
    }
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    let outcome = catch_unwind(AssertUnwindSafe(|| match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools::list()),
        "tools/call" => tools::call_message(state, &params),
        #[cfg(test)]
        "test/panic" => panic!("test panic"),
        _ => Err(CallError::Protocol(RpcError {
            code: -32601,
            message: format!("method not found: {method}"),
        })),
    }));
    Some(match outcome {
        Ok(Ok(result)) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Ok(Err(CallError::Protocol(error))) => error_response(id, error),
        Ok(Err(CallError::Tool(error))) => tool_error(id, &error),
        Err(_) => {
            // A half-changed document is unsafe to keep, so the save is closed without writing.
            state.session = None;
            if method == "tools/call" {
                let error = ApiError::new(
                    app_codes::INTERNAL,
                    PANIC_MESSAGE,
                    "Open the save again and repeat the last step.",
                );
                tool_error(id, &error)
            } else {
                error_response(
                    id,
                    RpcError {
                        code: -32603,
                        message: PANIC_MESSAGE.to_string(),
                    },
                )
            }
        }
    })
}

/// The `initialize` result: protocol version, capabilities, server info and the instructions.
fn initialize(params: &Value) -> Value {
    let wanted = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .filter(|version| PROTOCOL_VERSIONS.contains(version));
    let protocol = wanted.unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": protocol,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {
            "name": "saveprism",
            "title": "SavePrism",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": INSTRUCTIONS
    })
}

/// A JSON-RPC error response.
fn error_response(id: Value, error: RpcError) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": error.value()})
}

/// A tool result that carries an [`ApiError`] as text and is marked as an error.
fn tool_error(id: Value, error: &ApiError) -> Value {
    let text = json!({
        "code": error.code,
        "message": error.message,
        "hint": error.hint
    });
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": text.to_string()}],
            "isError": true
        }
    })
}

/// The JSON-RPC `-32602` error for a malformed `tools/call`.
fn bad_arguments(message: impl Into<String>) -> CallError {
    CallError::Protocol(RpcError::invalid(message))
}

/// The app's error for an argument value the tool cannot use (such as an unknown section).
fn bad_value(message: impl Into<String>, hint: impl Into<String>) -> CallError {
    CallError::Tool(ApiError::new(app_codes::MCP_ARGUMENT, message, hint))
}
