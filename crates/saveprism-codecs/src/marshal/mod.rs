//! `format.ruby-marshal`: Ruby Marshal data (format 4.8), the save format of RPG Maker XP
//! (`.rxdata`), VX (`.rvdata`) and VX Ace (`.rvdata2`).
//!
//! Written from the public format description (`doc/language/marshal.rdoc` in the Ruby
//! repository). The file is parsed to a data tree only: no class is looked up, no `_load`,
//! `marshal_load` or `_load_data` runs, nothing in the file is executed.
//!
//! # Tree
//!
//! An RPG Maker save is several Marshal dumps written one after another (XP and VX write one dump
//! per game object, VX Ace a header hash and then a contents hash). The root is an `Array` of the
//! dumps (hint `RUBY:dumps`). Bytes after the last dump that do not start a new dump are kept as a
//! read-only `Bytes` node (hint `RUBY:trailing`) at the end of that array.
//!
//! Every node carries a hint `RUBY:<kind>` or `RUBY:<kind>:<argument>`:
//!
//! | Ruby | Node | Hint |
//! |---|---|---|
//! | `nil` | `Null` (read-only) | `RUBY:nil` |
//! | `true` / `false` | `Bool` | `RUBY:bool` |
//! | Fixnum, Bignum | `Int` (no fixed width) | `RUBY:int` |
//! | Bignum larger than 128 bits | `Bytes` (read-only, the raw payload) | `RUBY:bignum` |
//! | Float | `Float` (the original text is the lexeme when it differs from ours) | `RUBY:float` |
//! | String | `Str` (UTF-8) or `Bytes` (read-only) | `RUBY:string`, `RUBY:string/utf8` (`:E => true`), `RUBY:string/ascii` (`:E => false`) |
//! | Symbol | `Str` (read-only) | `RUBY:symbol` |
//! | Regexp | `Str` / `Bytes` (read-only) | `RUBY:regexp[/utf8\|/ascii]:<options byte>` |
//! | Class / Module reference | `Str` / `Bytes` (read-only) | `RUBY:class`, `RUBY:module`, `RUBY:oldmodule` |
//! | `_dump` data (`Table`, `Color`, `Tone`) | `Bytes` (read-only) | `RUBY:userdef[/utf8\|/ascii]:<Class>` |
//! | Array | `Array` | `RUBY:array` |
//! | Hash | `Object` keyed by symbol name when every key is a symbol, else `Map` | `RUBY:hash` |
//! | Hash with default | as Hash; the last entry is the default (key `(default)`) | `RUBY:hash+default` |
//! | Object | `Object` of instance variables (`@gold`, ...) | `RUBY:object:<Class>` |
//! | Struct | `Object` of members | `RUBY:struct:<Class>` |
//! | `marshal_dump` data, Data object | `Object` with one entry `(data)` | `RUBY:usermarshal:<Class>`, `RUBY:data:<Class>` |
//! | Subclass of String/Array/Hash/Regexp | `Object` with one entry `(value)` | `RUBY:userclass:<Class>` |
//! | Object extended by a module | `Object` with one entry `(value)` | `RUBY:extended:<Module>` |
//! | Other instance variables (`I`) | `Object`: `(value)` then the variables | `RUBY:ivars` |
//! | Object link (`@`) | `Ref` to the linked node (read-only) | `RUBY:link` |
//!
//! Containers are read-only (no insert, remove or rename), so an edit never changes the shape.
//! Map keys are read-only. Editable: integers, floats, booleans and UTF-8 strings.
//!
//! One exception: a plain hash (no default) whose keys are all integers, or an empty hash, is
//! growable (`NodeFlags::GROWABLE`). [`insert_int_entry`] adds an `Int => Int` entry to its end,
//! the way the game's `@items[id] = n` does (RPG Maker party items, weapons and armors). Integers
//! write no symbol and no float, so the recorded symbol and float numbering stays right, and
//! object links are numbered from the tree, so a Bignum entry keeps every link right too. Only an
//! entry added this way can be taken out again.
//!
//! # Exact writing
//!
//! The writer is canonical (what Ruby writes) and the core round-trip guard compares the bytes.
//! Object links are written from the tree (`Ref` target numbered in writing order), so changing an
//! integer between the Fixnum and the Bignum form keeps every link right. What the canonical writer
//! cannot know is recorded:
//!
//! - integers stored in a non-canonical form: the original bytes as the lexeme `raw:<hex>`
//!   (dropped when the value is edited);
//! - float text that differs from ours: the text as the lexeme; extra mantissa bytes after a NUL
//!   (older writers) in the parameter `float_tails` (float number in file order → bytes);
//! - symbols: `symbol_redefs` (occurrences written with `:` although already defined),
//!   `symbol_links` (occurrence → table index when not the first definition), `symbol_utf8` /
//!   `symbol_ascii` (definitions written as `I:` with `:E => true/false`). Occurrences are counted
//!   over the whole file in order; symbols are read-only, so the count never changes.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Edit, Error, ErrorCode, Limits, Node, NodeFlags,
    OwnedValue, Params, Path, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value, apply_edits,
    codes,
};

/// The data is truncated or malformed (bad type byte, bad length, bad float text).
pub const RUBY_PARSE: ErrorCode = ErrorCode::new("SP-RUBY-001");
/// An object link (`@`) or symbol link (`;`) points outside its table.
pub const RUBY_BAD_LINK: ErrorCode = ErrorCode::new("SP-RUBY-002");
/// A construct this codec does not read (unsupported version, non-UTF-8 symbol name, ...).
pub const RUBY_UNSUPPORTED: ErrorCode = ErrorCode::new("SP-RUBY-003");
/// The document cannot be written as Marshal data (a value changed its type or shape).
pub const RUBY_WRITE: ErrorCode = ErrorCode::new("SP-RUBY-004");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "RUBY:";

/// Key of the default value of a hash with a default.
pub const DEFAULT_KEY: &str = "(default)";
/// Key of the wrapped value of `userclass`, `extended` and `ivars` nodes.
pub const VALUE_KEY: &str = "(value)";
/// Key of the data of `usermarshal` and `data` nodes.
pub const DATA_KEY: &str = "(data)";

const VERSION: [u8; 2] = [4, 8];
/// Stack of the thread that reads or writes: both recurse once per nesting level.
const STACK_BYTES: usize = 64 * 1024 * 1024;
/// Deepest nesting read or written whatever the limits say (keeps the recursion on that stack).
pub(crate) const MAX_NESTING: usize = 4096;
const EXTENSIONS: [&str; 3] = [".rxdata", ".rvdata", ".rvdata2"];

/// Registers the Ruby Marshal codec as `format.ruby-marshal`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(RubyMarshalCodec));
}

/// The `format.ruby-marshal` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct RubyMarshalCodec;

/// The encoding instance variable folded into a string, regexp or `_dump` hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Enc {
    /// No `I` wrapper (binary, or Ruby 1.8).
    Binary,
    /// `:E => true` (UTF-8).
    Utf8,
    /// `:E => false` (US-ASCII).
    Ascii,
}

impl Enc {
    fn suffix(self) -> &'static str {
        match self {
            Enc::Binary => "",
            Enc::Utf8 => "/utf8",
            Enc::Ascii => "/ascii",
        }
    }
}

/// The kind of a node, read from its hint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind<'a> {
    Dumps,
    Trailing,
    Nil,
    Bool,
    Int,
    Bignum,
    Float,
    Str(Enc),
    Symbol,
    Regexp(Enc, u8),
    Class,
    Module,
    OldModule,
    UserDef(Enc, &'a str),
    Array,
    Hash,
    HashDefault,
    DefaultKey,
    Object(&'a str),
    Struct(&'a str),
    UserMarshal(&'a str),
    Data(&'a str),
    UserClass(&'a str),
    Extended(&'a str),
    Ivars,
    Link,
}

fn split_enc(kind: &str) -> (&str, Option<Enc>) {
    match kind.split_once('/') {
        None => (kind, Some(Enc::Binary)),
        Some((base, "utf8")) => (base, Some(Enc::Utf8)),
        Some((base, "ascii")) => (base, Some(Enc::Ascii)),
        Some((base, _)) => (base, None),
    }
}

/// Parses a `RUBY:` hint; `None` for anything this codec did not write.
pub(crate) fn parse_hint(hint: &str) -> Option<Kind<'_>> {
    let rest = hint.strip_prefix(HINT_PREFIX)?;
    let (kind, arg) = match rest.split_once(':') {
        Some((kind, arg)) => (kind, Some(arg)),
        None => (rest, None),
    };
    let (base, enc) = split_enc(kind);
    let enc = enc?;
    let plain = enc == Enc::Binary;
    Some(match (base, arg) {
        ("dumps", None) if plain => Kind::Dumps,
        ("trailing", None) if plain => Kind::Trailing,
        ("nil", None) if plain => Kind::Nil,
        ("bool", None) if plain => Kind::Bool,
        ("int", None) if plain => Kind::Int,
        ("bignum", None) if plain => Kind::Bignum,
        ("float", None) if plain => Kind::Float,
        ("string", None) => Kind::Str(enc),
        ("symbol", None) if plain => Kind::Symbol,
        ("regexp", Some(options)) => Kind::Regexp(enc, options.parse().ok()?),
        ("class", None) if plain => Kind::Class,
        ("module", None) if plain => Kind::Module,
        ("oldmodule", None) if plain => Kind::OldModule,
        ("userdef", Some(class)) => Kind::UserDef(enc, class),
        ("array", None) if plain => Kind::Array,
        ("hash", None) if plain => Kind::Hash,
        ("hash+default", None) if plain => Kind::HashDefault,
        ("default", None) if plain => Kind::DefaultKey,
        ("object", Some(class)) if plain => Kind::Object(class),
        ("struct", Some(class)) if plain => Kind::Struct(class),
        ("usermarshal", Some(class)) if plain => Kind::UserMarshal(class),
        ("data", Some(class)) if plain => Kind::Data(class),
        ("userclass", Some(class)) if plain => Kind::UserClass(class),
        ("extended", Some(module)) if plain => Kind::Extended(module),
        ("ivars", None) if plain => Kind::Ivars,
        ("link", None) if plain => Kind::Link,
        _ => return None,
    })
}

/// The Ruby class (or module) named by the hint of `node`: objects, structs, `_dump` and
/// `marshal_dump` data, Data objects, user classes and extended objects. `None` otherwise.
pub fn ruby_class(node: &Node) -> Option<&str> {
    match parse_hint(node.type_hint.as_ref()?.as_str())? {
        Kind::Object(class)
        | Kind::Struct(class)
        | Kind::UserDef(_, class)
        | Kind::UserMarshal(class)
        | Kind::Data(class)
        | Kind::UserClass(class)
        | Kind::Extended(class) => Some(class),
        _ => None,
    }
}

/// True when `node` is a growable Ruby hash: a plain hash (no default) with integer keys only, or
/// an empty one. [`insert_int_entry`] adds to it.
pub fn is_growable_hash(node: &Node) -> bool {
    let plain = node
        .type_hint
        .as_ref()
        .and_then(|hint| parse_hint(hint.as_str()))
        == Some(Kind::Hash);
    plain && node.flags.contains(NodeFlags::GROWABLE) && matches!(node.value, Value::Map(_))
}

/// Adds the entry `key => value` (two integers) to the end of the growable hash at `parent` and
/// returns the entry's index, as Ruby's `hash[key] = value` does for a new key. The new nodes get
/// the hints the writer needs; the key is read-only like every hash key. Fails on any other node
/// and on a key the hash already has. Atomic: on an error the document is unchanged.
pub fn insert_int_entry(
    doc: &mut SaveDoc,
    parent: &Path,
    key: i128,
    value: i128,
    limits: &Limits,
) -> Result<usize> {
    let growable = doc
        .resolve(parent)
        .and_then(|id| doc.get(id))
        .is_some_and(is_growable_hash);
    if !growable {
        return Err(Error::new(
            codes::CORE_READ_ONLY,
            format!(
                "the value at '{parent}' takes no new entries (only a hash with whole-number keys does)"
            ),
        ));
    }
    let mut work = doc.clone();
    apply_edits(
        &mut work,
        &[Edit::InsertEntry {
            parent: parent.clone(),
            key: OwnedValue::Int(key),
            value: OwnedValue::Int(value),
        }],
        limits,
    )?;
    let count = match work.resolve(parent).and_then(|id| work.get(id)) {
        Some(Node {
            value: Value::Map(entries),
            ..
        }) => entries.len(),
        _ => 0,
    };
    let index = count
        .checked_sub(1)
        .ok_or_else(|| Error::new(RUBY_WRITE, "the new hash entry is missing"))?;
    let entry = |suffix: &str| {
        let mut path = parent.clone();
        path.push(format!("{index}{suffix}"));
        work.resolve(&path)
            .ok_or_else(|| Error::new(RUBY_WRITE, "the new hash entry is missing"))
    };
    let (key_id, value_id) = (entry(".key")?, entry("")?);
    let int = TypeHint::new(format!("{HINT_PREFIX}int"));
    work.set_type_hint(key_id, Some(int.clone()))?;
    work.set_type_hint(value_id, Some(int))?;
    let mut flags = NodeFlags::READ_ONLY;
    flags.insert(NodeFlags::SYNTHETIC);
    work.set_flags(key_id, flags)?;
    *doc = work;
    Ok(index)
}

/// True for a byte that can start a Marshal value.
fn is_type_byte(byte: u8) -> bool {
    b"0TFil\"f:;@[{}oSICeuU/cmMd".contains(&byte)
}

impl Codec for RubyMarshalCodec {
    fn id(&self) -> &'static str {
        "format.ruby-marshal"
    }

    /// The version header `04 08` followed by a type byte; the RPG Maker extensions raise the score.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let magic = input.starts_with(&VERSION) && input.get(2).copied().is_some_and(is_type_byte);
        if !magic {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        if named {
            Confidence::new(0.95)
        } else {
            Confidence::HIGH
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let shared: &DecodeCtx<'_> = ctx;
        let (doc, params) = on_big_stack(|| read::read_all(input, shared))?;
        ctx.charge_nodes(len_u64(doc.len()))?;
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
            .name("ruby-marshal".to_string())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)
            .map_err(|source| {
                Error::new(RUBY_PARSE, "could not start the Marshal worker thread")
                    .with_source(source)
            })?;
        handle.join().unwrap_or_else(|_| {
            Err(Error::new(
                RUBY_PARSE,
                "the Marshal worker stopped unexpectedly",
            ))
        })
    })
}
