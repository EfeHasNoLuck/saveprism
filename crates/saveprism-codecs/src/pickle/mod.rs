//! `format.python-pickle`: Python pickle data, protocols 0 to 5 (Ren'Py saves and `persistent`).
//!
//! Written from the public description of the pickle opcodes (`Lib/pickletools.py` documentation
//! and PEP 307, 3154 and 574). The data is never unpickled: a small machine runs the opcodes on a
//! stack and builds a graph of plain data. No module is imported, no class is looked up, nothing
//! is constructed or called. `GLOBAL` / `STACK_GLOBAL` only give a class name.
//!
//! # Tree
//!
//! The root is the unpickled top-level object. Every node carries a hint `PY:<kind>` or
//! `PY:<kind>:<module.Class>`:
//!
//! | Python | Node | Hint |
//! |---|---|---|
//! | `None` | `Null` (read-only) | `PY:none` |
//! | `bool` | `Bool` | `PY:bool` |
//! | `int` | `Int` (no fixed width) | `PY:int` |
//! | `int` beyond 128 bits | `Str` decimal text (read-only) | `PY:bigint` |
//! | `float` | `Float` | `PY:float` |
//! | `str` (Python 2 `unicode`) | `Str` | `PY:str` |
//! | Python 2 `str` | `Str` when UTF-8, else `Bytes` (read-only) | `PY:bytestr` |
//! | `bytes`, `bytearray` | `Bytes` (read-only) | `PY:bytes`, `PY:bytearray` |
//! | `tuple`, `list` | `Array` | `PY:tuple`, `PY:list` |
//! | `dict` | `Object` when every key is text, else `Map` (read-only keys) | `PY:dict` |
//! | `set`, `frozenset` | `Array` (read-only members) | `PY:set`, `PY:frozenset` |
//! | class reference | `Str` `module.name` (read-only) | `PY:global` |
//! | object with an attribute dict | `Object` of the attributes | `PY:object:<Class>` |
//! | list / dict / set subclass (`RevertableList`, ...) | `Array` / `Object` / `Array` | `PY:list:<Class>`, `PY:dict:<Class>`, `PY:set:<Class>` |
//! | any other made object | `Object` with `(args)`, `(kwargs)`, `(state)`, `(items)`, `(entries)`, `(members)`, `(callable)` | `PY:instance:<Class>` |
//! | an object seen before (memo `GET`, `DUP`) | `Ref` to its first node (read-only) | `PY:ref` |
//! | persistent id, extension code | `Str` / `Object` / `Int` (read-only) | `PY:persid`, `PY:ext` |
//!
//! A Ren'Py `RevertableSet` (state `({member: True, ...},)`) shows its members as a `PY:set:` array.
//! Objects of protocol 0 and 1 (`copyreg._reconstructor`) show the class of their first argument.
//!
//! Read-only: containers keep their shape (no insert, remove or rename); constructor arguments, dict
//! keys and set members; every value referred to more than once (an edit would change all of them);
//! text that cannot be written back the same way. Editable: `int`, `float`, `bool` and `str` values
//! that appear once.
//!
//! # Exact writing
//!
//! The original opcode stream is kept in the parameter `stream`. Encoding decodes it again, walks
//! both trees side by side (the shape must be the same) and replaces only the opcodes of the edited
//! scalars (the node `span`). A new value may need a wider opcode (`BININT1` to `BININT` or
//! `LONG1`, `SHORT_BINUNICODE` to `BINUNICODE`); protocol 4+ `FRAME` lengths are recomputed. An
//! unchanged document gives the original bytes.

mod ops;
mod tree;
mod vm;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Limits, Node, Params, Registry,
    Result, SaveDoc, SniffCtx,
};

/// The data is truncated or malformed (bad opcode, bad length, bad text line).
pub const PICKLE_PARSE: ErrorCode = ErrorCode::new("SP-PICKLE-001");
/// A memo `GET` names an entry that was never `PUT`.
pub const PICKLE_BAD_MEMO: ErrorCode = ErrorCode::new("SP-PICKLE-002");
/// A construct this codec does not read (protocol above 5, out-of-band buffers).
pub const PICKLE_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-PICKLE-003");
/// The document cannot be written back as this pickle (a shape or type changed).
pub const PICKLE_WRITE: ErrorCode = ErrorCode::new("SP-PICKLE-004");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "PY:";
/// Key of the constructor arguments of an object.
pub const ARGS_KEY: &str = "(args)";
/// Key of the keyword arguments of an object (`NEWOBJ_EX`).
pub const KWARGS_KEY: &str = "(kwargs)";
/// Key of the `BUILD` state of an object.
pub const STATE_KEY: &str = "(state)";
/// Key of the appended items of an object.
pub const ITEMS_KEY: &str = "(items)";
/// Key of the set items of an object.
pub const ENTRIES_KEY: &str = "(entries)";
/// Key of the added members of an object.
pub const MEMBERS_KEY: &str = "(members)";
/// Key of a callable that is not a class name.
pub const CALLABLE_KEY: &str = "(callable)";
/// Key of the id of a persistent reference.
pub const ID_KEY: &str = "(id)";

/// Parameter holding the original opcode stream.
const STREAM_PARAM: &str = "stream";
/// Stack of the thread that builds the tree (it recurses once per nesting level).
const STACK_BYTES: usize = 64 * 1024 * 1024;
/// Deepest nesting read whatever the limits say (keeps the recursion on that stack).
pub(crate) const MAX_NESTING: usize = 4096;

/// Registers the pickle codec as `format.python-pickle`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(PythonPickleCodec));
}

/// The `format.python-pickle` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct PythonPickleCodec;

/// The class named by the hint of `node` (`PY:object:store.Inventory` gives `store.Inventory`).
pub fn python_class(node: &Node) -> Option<&str> {
    let rest = node
        .type_hint
        .as_ref()?
        .as_str()
        .strip_prefix(HINT_PREFIX)?;
    let (kind, class) = rest.split_once(':')?;
    matches!(kind, "object" | "list" | "dict" | "set" | "instance").then_some(class)
}

/// Decodes a whole stream into its tree and its protocol (0 when there is no `PROTO`). Recurses
/// once per nesting level: run it on a big stack.
fn decode_stream(input: &[u8], limits: &Limits) -> Result<(SaveDoc, u8)> {
    let graph = vm::run(input, limits)?;
    let ctx = DecodeCtx::new(limits);
    let doc = tree::build(&graph, &ctx)?;
    Ok((doc, graph.proto))
}

/// True for a byte that can start a pickle of protocol 0 or 1.
fn starts_old_pickle(byte: u8) -> bool {
    b"(]})NIJKLMFGSTUVXcde".contains(&byte)
}

impl Codec for PythonPickleCodec {
    fn id(&self) -> &'static str {
        "format.python-pickle"
    }

    /// `PROTO 2..=5` at the start and `STOP` at the end; a protocol 0/1 look-alike scores low.
    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.last() != Some(&b'.') {
            return Confidence::NONE;
        }
        match (input.first(), input.get(1)) {
            (Some(0x80), Some(2..=5)) => Confidence::HIGH,
            (Some(&first), _) if starts_old_pickle(first) && input.len() >= 2 => Confidence::LOW,
            _ => Confidence::NONE,
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let limits = ctx.limits().clone();
        let (doc, _) = on_big_stack(|| decode_stream(input, &limits))?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set(STREAM_PARAM, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        on_big_stack(|| write::encode(doc, params))
    }
}

/// Runs `work` on a thread with a large stack, so deep (but allowed) nesting cannot overflow the
/// caller's stack.
fn on_big_stack<T: Send>(work: impl FnOnce() -> Result<T> + Send) -> Result<T> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("python-pickle".to_string())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|source| {
                Error::new(PICKLE_PARSE, "could not start the pickle worker thread")
                    .with_source(source)
            })?;
        handle.join().unwrap_or_else(|_| {
            Err(Error::new(
                PICKLE_PARSE,
                "the pickle worker stopped unexpectedly",
            ))
        })
    })
}
