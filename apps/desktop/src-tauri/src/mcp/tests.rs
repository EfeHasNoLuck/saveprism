//! Tests of the MCP server in process: byte buffers in, byte buffers out, a temporary folder for
//! the saves and backups.

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use saveprism_codecs::terraria::aes::TerrariaPlayerLayer;
use saveprism_core::{Layer, Params};
use serde_json::{Value, json};

use super::serve;

const PASSWORD: &str = "synthetic-test-key";

/// A work directory with a backup directory next to it.
struct Env {
    _root: tempfile::TempDir,
    work: PathBuf,
    backups: PathBuf,
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let work = root.path().join("work");
    let backups = root.path().join("backups");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(&backups).unwrap();
    Env {
        _root: root,
        work,
        backups,
    }
}

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures")
        .join(relative)
}

fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn tool_call(id: i64, name: &str, arguments: Value) -> Value {
    request(
        id,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    )
}

/// Runs `lines` through `serve` and returns the raw output text.
fn run_raw(lines: &[String], backup_root: Option<PathBuf>) -> String {
    let mut input = Vec::new();
    for line in lines {
        input.extend_from_slice(line.as_bytes());
        input.push(b'\n');
    }
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, backup_root).unwrap();
    String::from_utf8(output).unwrap()
}

/// Runs raw input bytes through `serve` and returns the parsed reply objects, in order.
fn run_bytes(input: &[u8], backup_root: Option<PathBuf>) -> Vec<Value> {
    let mut output = Vec::new();
    serve(Cursor::new(input), &mut output, backup_root).unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// Runs the requests and returns the parsed reply objects, in order.
fn run(requests: &[Value], backup_root: Option<PathBuf>) -> Vec<Value> {
    let lines: Vec<String> = requests
        .iter()
        .map(|request| serde_json::to_string(request).unwrap())
        .collect();
    run_raw(&lines, backup_root)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn reply(responses: &[Value], id: i64) -> &Value {
    responses
        .iter()
        .find(|response| response["id"] == id)
        .unwrap_or_else(|| panic!("no reply {id}"))
}

/// The JSON of a tool result, parsed from its text content.
fn tool_json(response: &Value) -> Value {
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    serde_json::from_str(text).unwrap()
}

fn is_tool_error(response: &Value) -> bool {
    response["result"]["isError"] == true
}

fn write_save(env: &Env) -> PathBuf {
    let path = env.work.join("save.json");
    fs::write(&path, br#"{"name":"Ada","level":1,"gold":5}"#).unwrap();
    path
}

/// A save whose value list is far over the result cap when read in one page.
fn write_big_save(env: &Env) -> PathBuf {
    let path = env.work.join("big.json");
    let mut object = serde_json::Map::new();
    for index in 0..600 {
        object.insert(format!("key{index:04}"), json!("x".repeat(400)));
    }
    fs::write(&path, serde_json::to_vec(&Value::Object(object)).unwrap()).unwrap();
    path
}

#[test]
fn initialize_echoes_a_known_version_and_falls_back_to_the_newest() {
    let responses = run(
        &[
            request(1, "initialize", json!({"protocolVersion": "2025-03-26"})),
            request(2, "initialize", json!({"protocolVersion": "1999-01-01"})),
            request(3, "initialize", json!({})),
        ],
        None,
    );
    let known = tool_json_version(&responses, 1);
    assert_eq!(known, "2025-03-26");
    assert_eq!(tool_json_version(&responses, 2), "2025-11-25");
    assert_eq!(tool_json_version(&responses, 3), "2025-11-25");
    let result = &reply(&responses, 1)["result"];
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    assert_eq!(result["serverInfo"]["name"], "saveprism");
    assert_eq!(result["serverInfo"]["title"], "SavePrism");
    assert_eq!(result["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(
        result["instructions"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
}

fn tool_json_version(responses: &[Value], id: i64) -> String {
    reply(responses, id)["result"]["protocolVersion"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_notification_gets_no_reply() {
    let lines = vec![
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
        request(2, "ping", json!({})).to_string(),
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 2}})
            .to_string(),
    ];
    let responses: Vec<Value> = run_raw(&lines, None)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["id"], 2);
    assert_eq!(responses[0]["result"], json!({}));
}

#[test]
fn tools_list_has_sixteen_tools_with_object_schemas() {
    let responses = run(&[request(1, "tools/list", json!({}))], None);
    let tools = reply(&responses, 1)["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 16);
    for tool in tools {
        assert!(tool["name"].as_str().is_some());
        assert!(tool["title"].as_str().is_some());
        assert!(tool["description"].as_str().is_some());
        assert!(tool["inputSchema"].is_object(), "{tool}");
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert!(tool["inputSchema"]["properties"].is_object());
        assert!(tool["inputSchema"]["required"].is_array());
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["annotations"].is_object());
        assert_eq!(tool["annotations"]["openWorldHint"], false);
    }
}

#[test]
fn protocol_errors_use_the_right_codes() {
    let lines = vec![
        request(1, "does/not/exist", json!({})).to_string(),
        "{not json".to_string(),
        "[1,2]".to_string(),
        tool_call(2, "no_such_tool", json!({})).to_string(),
        tool_call(3, "list_values", json!({"path": 5})).to_string(),
    ];
    let responses: Vec<Value> = run_raw(&lines, None)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(responses[0]["error"]["code"], -32601);
    assert_eq!(responses[1]["error"]["code"], -32700);
    assert_eq!(responses[1]["id"], Value::Null);
    assert_eq!(responses[2]["error"]["code"], -32600);
    assert_eq!(responses[2]["id"], Value::Null);
    assert_eq!(responses[3]["error"]["code"], -32602);
    assert_eq!(responses[4]["error"]["code"], -32602);
}

#[test]
fn a_tool_before_open_save_is_a_tool_error() {
    let responses = run(&[tool_call(1, "list_values", json!({}))], None);
    assert!(is_tool_error(&responses[0]));
    assert_eq!(tool_json(&responses[0])["code"], "SP-APP-002");
}

#[test]
fn the_full_flow_reads_edits_and_saves_with_a_backup() {
    let env = env();
    let path = write_save(&env);
    let requests = vec![
        tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
        tool_call(2, "list_values", json!({"path": ""})),
        tool_call(3, "search_values", json!({"query": "Ada"})),
        tool_call(
            4,
            "propose_edits",
            json!({"edits": [
                {"path": "/level", "value": "42"},
                {"path": "/gold", "value": "99"}
            ]}),
        ),
        tool_call(5, "pending_changes", json!({})),
        tool_call(6, "save_changes", json!({})),
    ];
    let responses = run(&requests, Some(env.backups.clone()));
    for id in 1..=6 {
        assert!(
            !is_tool_error(reply(&responses, id)),
            "{id}: {}",
            reply(&responses, id)
        );
    }
    assert_eq!(tool_json(reply(&responses, 1))["pendingCount"], 0);
    let page = tool_json(reply(&responses, 2));
    assert_eq!(page["total"], 3);
    let hits = tool_json(reply(&responses, 3));
    assert_eq!(hits["total"], 1);
    let changed = tool_json(reply(&responses, 4));
    assert_eq!(changed["changedCount"], 2);
    assert_eq!(changed["pendingCount"], 2);
    assert_eq!(changed["pendingChanges"].as_array().unwrap().len(), 2);
    let pending = tool_json(reply(&responses, 5));
    assert_eq!(pending["total"], 2);
    assert_eq!(pending["offset"], 0);
    assert_eq!(pending["items"].as_array().unwrap().len(), 2);
    let saved = tool_json(reply(&responses, 6));
    assert_eq!(saved["verified"], true);
    assert!(saved["backupPath"].as_str().is_some());

    let on_disk: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(on_disk["level"], 42);
    assert_eq!(on_disk["gold"], 99);
    assert!(fs::read_dir(&env.backups).unwrap().next().is_some());
}

#[test]
fn propose_edits_with_one_bad_value_changes_nothing() {
    let env = env();
    let path = write_save(&env);
    let before = fs::read(&path).unwrap();
    let requests = vec![
        tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
        tool_call(
            2,
            "propose_edits",
            json!({"edits": [
                {"path": "/level", "value": "7"},
                {"path": "/gold", "value": "not a number"}
            ]}),
        ),
        tool_call(3, "pending_changes", json!({})),
    ];
    let responses = run(&requests, Some(env.backups.clone()));
    assert!(is_tool_error(reply(&responses, 2)));
    let pending = tool_json(reply(&responses, 3));
    assert_eq!(pending["total"], 0);
    assert_eq!(pending["items"], json!([]));
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn game_view_reads_an_rpg_maker_mv_save_by_section_and_filter() {
    let env = env();
    let source = fixture("synthetic/rpgmaker-mv/file1.rpgsave");
    let path = env.work.join("file1.rpgsave");
    fs::copy(&source, &path).unwrap();
    let requests = vec![
        tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
        tool_call(2, "game_view", json!({})),
        tool_call(
            3,
            "game_view",
            json!({"section": "variables", "filter": "1"}),
        ),
        tool_call(4, "game_view", json!({"section": "bogus"})),
        tool_call(
            5,
            "game_view",
            json!({"section": "variables", "filter": ""}),
        ),
        tool_call(
            6,
            "game_view",
            json!({"section": "variables", "filter": "   ", "limit": 500}),
        ),
    ];
    let responses = run(&requests, Some(env.backups.clone()));
    for id in [1, 2, 3, 5, 6] {
        assert!(
            !is_tool_error(reply(&responses, id)),
            "{id}: {}",
            reply(&responses, id)
        );
    }
    let summary = tool_json(reply(&responses, 2));
    assert_eq!(summary["kind"], "rpgmaker");
    assert_eq!(summary["engine"], "mv");
    assert!(summary["counts"]["variables"].as_u64().unwrap() >= 1);
    assert!(summary["counts"]["items"].as_u64().unwrap() >= 1);
    let variables = tool_json(reply(&responses, 3));
    assert_eq!(variables["section"], "variables");
    let rows = variables["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row["id"] == 1));
    assert_eq!(variables["total"], rows.len());
    assert!(is_tool_error(reply(&responses, 4)));
    let blank = tool_json(reply(&responses, 5));
    assert_eq!(blank["total"], summary["counts"]["variables"]);
    assert!(!blank["rows"].as_array().unwrap().is_empty());
    let spaces = tool_json(reply(&responses, 6));
    assert_eq!(spaces["total"], summary["counts"]["variables"]);
    assert_eq!(tool_json(reply(&responses, 4))["code"], "SP-APP-024");
}

#[test]
fn the_password_never_appears_in_any_output_line() {
    let path = fixture("keyed/es3-aes/settings.es3");
    let requests = [
        request(1, "initialize", json!({})),
        tool_call(
            2,
            "open_save",
            json!({"path": path.to_str().unwrap(), "password": PASSWORD}),
        ),
        tool_call(3, "list_values", json!({})),
        request(4, "tools/list", json!({})),
    ];
    let lines: Vec<String> = requests
        .iter()
        .map(|request| serde_json::to_string(request).unwrap())
        .collect();
    let output = run_raw(&lines, None);
    assert!(!output.contains(PASSWORD));
    let responses: Vec<Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!is_tool_error(reply(&responses, 2)));
}

#[test]
fn a_terraria_key_never_appears_in_any_output_line() {
    // The key found for (or given with) a `.plr` must stay as quiet as an Easy Save 3 password.
    // Only a key passed in the call can be tested here: the tool offers no way to point the
    // automatic key search at a fake game folder.
    const TERRARIA_KEY: &str = "SynthKy8";
    let env = env();
    let plain = fs::read(fixture("synthetic/terraria/player-decrypted.plr")).unwrap();
    let mut params = Params::new();
    params.set("password", TERRARIA_KEY);
    let encrypted = TerrariaPlayerLayer.encode(&plain, &params).unwrap();
    let path = env.work.join("Mira.plr");
    fs::write(&path, encrypted).unwrap();

    let requests = [
        request(1, "initialize", json!({})),
        tool_call(
            2,
            "open_save",
            json!({"path": path.to_str().unwrap(), "password": TERRARIA_KEY}),
        ),
        tool_call(3, "list_values", json!({})),
        tool_call(4, "search_values", json!({"query": "Mira"})),
        request(5, "tools/list", json!({})),
    ];
    let lines: Vec<String> = requests
        .iter()
        .map(|request| serde_json::to_string(request).unwrap())
        .collect();
    let output = run_raw(&lines, Some(env.backups.clone()));
    assert!(!output.contains(TERRARIA_KEY));
    let responses: Vec<Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!is_tool_error(reply(&responses, 2)));
    assert_eq!(
        tool_json(reply(&responses, 2))["codec"],
        "format.terraria-player"
    );
    assert!(!is_tool_error(reply(&responses, 3)));
    assert!(!is_tool_error(reply(&responses, 4)));
}

#[test]
fn a_line_that_is_not_utf8_gets_a_parse_error_and_the_server_goes_on() {
    let mut input = b"\xff\xfe\n".to_vec();
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\r\n");
    let responses = run_bytes(&input, None);
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["error"]["code"], -32700);
    assert_eq!(responses[1]["id"], 7);
    assert_eq!(responses[1]["result"], json!({}));
}

#[test]
fn a_byte_order_mark_before_a_line_is_ignored() {
    let responses = run_bytes(
        b"\xef\xbb\xbf{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}\n",
        None,
    );
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0]["result"], json!({}));
}

#[test]
fn a_result_over_the_cap_is_refused_with_a_hint() {
    let env = env();
    let path = write_big_save(&env);
    let responses = run(
        &[
            tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
            tool_call(2, "list_values", json!({"limit": 500})),
            tool_call(3, "list_values", json!({"limit": 10})),
        ],
        Some(env.backups.clone()),
    );
    let refused = reply(&responses, 2);
    assert!(is_tool_error(refused));
    assert_eq!(tool_json(refused)["code"], "SP-APP-024");
    assert!(!is_tool_error(reply(&responses, 3)));
}

#[test]
fn save_changes_writes_only_next_to_the_open_file() {
    let env = env();
    let path = write_save(&env);
    let other = env.work.parent().unwrap().join("elsewhere.json");
    let sibling = env.work.join("copy.json");
    let existing = env.work.join("existing.json");
    fs::write(&existing, b"{}").unwrap();
    let edit = json!({"edits": [{"path": "/gold", "value": "7"}]});
    let responses = run(
        &[
            tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
            tool_call(2, "propose_edits", edit.clone()),
            tool_call(3, "save_changes", json!({"target": "copy.json"})),
            tool_call(
                4,
                "save_changes",
                json!({"target": other.to_str().unwrap()}),
            ),
            tool_call(
                5,
                "save_changes",
                json!({"target": existing.to_str().unwrap()}),
            ),
            tool_call(
                6,
                "save_changes",
                json!({"target": sibling.to_str().unwrap()}),
            ),
        ],
        Some(env.backups.clone()),
    );
    assert_eq!(tool_json(reply(&responses, 3))["code"], "SP-APP-024");
    assert_eq!(tool_json(reply(&responses, 4))["code"], "SP-APP-024");
    assert!(!other.exists());
    assert_eq!(tool_json(reply(&responses, 5))["code"], "SP-APP-006");
    assert_eq!(fs::read(&existing).unwrap(), b"{}");
    assert!(
        !is_tool_error(reply(&responses, 6)),
        "{}",
        reply(&responses, 6)
    );
    let written: Value = serde_json::from_slice(&fs::read(&sibling).unwrap()).unwrap();
    assert_eq!(written["gold"], 7);
}

#[test]
fn a_panic_closes_the_save_and_the_server_goes_on() {
    let env = env();
    let path = write_save(&env);
    let responses = run(
        &[
            tool_call(1, "open_save", json!({"path": path.to_str().unwrap()})),
            request(2, "test/panic", json!({})),
            tool_call(3, "pending_changes", json!({})),
            request(4, "ping", json!({})),
        ],
        Some(env.backups.clone()),
    );
    assert_eq!(reply(&responses, 2)["error"]["code"], -32603);
    assert_eq!(tool_json(reply(&responses, 3))["code"], "SP-APP-002");
    assert_eq!(reply(&responses, 4)["result"], json!({}));
}
