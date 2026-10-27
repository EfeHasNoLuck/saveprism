//! `format.gvas`: Unreal Engine `.sav` (GVAS) saves through the `uesave` library.
//!
//! No GVAS parser of our own. Decode reads the bytes with `uesave`, serialises the `uesave::Save`
//! to JSON with `serde_json` and lets the `format.json` codec turn that text into a [`SaveDoc`].
//! Encode runs the other way: the JSON codec writes the document, `serde_json` rebuilds the
//! `uesave::Save` and `uesave` writes the bytes. Nothing in the file is ever executed.
//!
//! Parameters of the step:
//! - `types` (table): `uesave` type hints, property path to struct type name (for example
//!   `PlayerAttributes.Key` = `"Struct"`). They are read from the recipe hints on decode. When a
//!   map key/value or set element is a struct but the file does not name its type, the decode
//!   infers the missing hint (see [`infer`]) and stores the complete table back, so later decodes
//!   reuse exactly the same set.
//! - the JSON codec's own format profile (`indent`, `regular`, ...), kept as it is.
//!
//! Known limits: the nodes carry no `GVAS:<type>` hint (a codec cannot set a hint on a node of a
//! finished document with the current core API), and `uesave` only guarantees a byte-identical
//! write for the property types it knows; the pipeline's round-trip guard reports anything else.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, ParamValue, Params, Registry, Result,
    SaveDoc, SniffCtx, codes,
};
use uesave::{Save, SaveReader, Types};

use crate::json::JsonCodec;

mod infer;

/// `uesave` could not read the GVAS bytes (the error carries the byte offset).
pub const GVAS_PARSE: ErrorCode = ErrorCode::new("SP-GVAS-001");
/// The JSON form of the save could not be written or read back.
pub const GVAS_JSON: ErrorCode = ErrorCode::new("SP-GVAS-002");
/// `uesave` could not write the save.
pub const GVAS_WRITE: ErrorCode = ErrorCode::new("SP-GVAS-003");

const MAGIC: &[u8] = b"GVAS";
const TYPES_KEY: &str = "types";

/// Registers the GVAS codec as `format.gvas`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GvasCodec));
}

/// The `format.gvas` codec.
pub(crate) struct GvasCodec;

/// Reads and validates the `types` table from the step hints.
fn read_types(hints: &Params) -> Result<Params> {
    let mut stored = Params::new();
    match hints.get(TYPES_KEY) {
        None => {}
        Some(ParamValue::Table(table)) => {
            for (path, value) in table.iter() {
                match value {
                    ParamValue::Str(name) => {
                        stored.set(path, name.as_str());
                    }
                    _ => {
                        return Err(Error::new(
                            codes::CORE_INVALID_PARAM,
                            format!("type hint \"{path}\" must be a string (struct type name)"),
                        ));
                    }
                }
            }
        }
        Some(_) => {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                format!("parameter \"{TYPES_KEY}\" must be a table of path = type name"),
            ));
        }
    }
    Ok(stored)
}

/// Runs `f`, turning a panic inside `uesave` into an error (the input is untrusted).
///
/// requires `panic = "unwind"` (see the root Cargo.toml release profile)
fn guarded<T>(code: ErrorCode, what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(Error::new(
            code,
            format!("the GVAS library failed unexpectedly while {what}"),
        ))
    })
}

fn parse_save(input: &[u8], types: Types) -> Result<Save> {
    guarded(GVAS_PARSE, "reading the save", || {
        Save::read_with_types(&mut Cursor::new(input), types).map_err(|source| {
            Error::new(GVAS_PARSE, format!("cannot read the GVAS save: {source}"))
                .with_offset(len_u64(source.offset))
                .with_source(source)
        })
    })
}

/// Parses with unreadable properties kept as raw bytes, so the hint search can
/// still see the surrounding tree and the schema of the property that failed.
fn parse_save_lenient(input: &[u8], types: Types) -> Result<Save> {
    guarded(GVAS_PARSE, "reading the save", || {
        SaveReader::new()
            .error_to_raw(true)
            .types(types)
            .read(Cursor::new(input))
            .map_err(|source| {
                Error::new(GVAS_PARSE, format!("cannot read the GVAS save: {source}"))
                    .with_offset(len_u64(source.offset))
                    .with_source(source)
            })
    })
}

fn write_save(save: &Save) -> Result<Vec<u8>> {
    guarded(GVAS_WRITE, "writing the save", || {
        let mut out = Vec::new();
        save.write(&mut out).map_err(|source| {
            Error::new(GVAS_WRITE, format!("cannot write the GVAS save: {source}"))
                .with_source(source)
        })?;
        Ok(out)
    })
}

impl Codec for GvasCodec {
    fn id(&self) -> &'static str {
        "format.gvas"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(MAGIC) {
            Confidence::HIGH
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        let input_len = len_u64(input.len());
        ctx.limits().check_input(input_len)?;
        let stored = read_types(ctx.hints())?;

        let (save, stored) = infer::guarded_read(input, &stored)?;
        let json = serde_json::to_string(&save).map_err(|source| {
            Error::new(
                GVAS_JSON,
                format!("cannot turn the GVAS save into JSON: {source}"),
            )
            .with_source(source)
        })?;
        ctx.check_output(input_len, len_u64(json.len()))?;

        let mut decoded = JsonCodec.decode(json.as_bytes(), ctx)?;
        decoded.params.set(TYPES_KEY, stored);
        Ok(decoded)
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let text = JsonCodec.encode(doc, params)?;
        let save: Save = serde_json::from_slice(&text).map_err(|source| {
            Error::new(
                GVAS_JSON,
                format!("the document is not a valid GVAS save: {source}"),
            )
            .with_source(source)
        })?;
        write_save(&save)
    }
}
