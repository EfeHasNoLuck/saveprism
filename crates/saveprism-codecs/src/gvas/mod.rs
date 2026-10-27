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
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, FloatWidth, NodeId, ParamValue,
    Params, Path, Registry, Result, SaveDoc, SniffCtx, Value, codes,
};
use uesave::{
    PropertySchemas, PropertyTagDataPartial, PropertyType as GvasType, Save, SaveReader, Types,
};

use crate::json::JsonCodec;

mod details;
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

/// The property name without the `_<index>` suffix `uesave` adds to each key.
fn property_name(key: &str) -> &str {
    match key.rsplit_once('_') {
        Some((name, index))
            if !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            name
        }
        _ => key,
    }
}

/// Collects the nodes whose schema says `FloatProperty`, so they get the 32-bit width of the file
/// instead of the default 64-bit width of JSON numbers.
fn collect_f32(
    doc: &SaveDoc,
    schemas: &PropertySchemas,
    id: NodeId,
    data: &PropertyTagDataPartial,
    prefix: &str,
    out: &mut Vec<NodeId>,
) {
    let Some(node) = doc.get(id) else {
        return;
    };
    match (data, &node.value) {
        (PropertyTagDataPartial::Other(GvasType::FloatProperty), Value::Float { .. }) => {
            out.push(id);
        }
        (PropertyTagDataPartial::Array(inner), Value::Array(items)) => {
            for child in items {
                collect_f32(doc, schemas, *child, inner, prefix, out);
            }
        }
        (PropertyTagDataPartial::Set { key_type }, Value::Array(items)) => {
            for child in items {
                collect_f32(doc, schemas, *child, key_type, prefix, out);
            }
        }
        (PropertyTagDataPartial::Struct { .. }, Value::Object(entries)) => {
            for (key, child) in entries {
                let name = property_name(key);
                let path = format!("{prefix}.{name}");
                if let Some(tag) = schemas.get(&path) {
                    collect_f32(doc, schemas, *child, &tag.data, &path, out);
                }
            }
        }
        _ => {}
    }
}

/// Gives every `FloatProperty` node of a decoded save the `F32` width, with its value narrowed to
/// the nearest 32-bit float. The lexeme stays, so an unchanged save still writes byte for byte.
fn mark_float32(doc: &mut SaveDoc, schemas: &PropertySchemas) -> Result<()> {
    let Ok(path) = Path::parse("/root/properties") else {
        return Ok(());
    };
    let Some(root) = doc.resolve(&path) else {
        return Ok(());
    };
    let Some(Value::Object(entries)) = doc.get(root).map(|node| node.value.clone()) else {
        return Ok(());
    };
    let mut marked = Vec::new();
    for (key, child) in entries {
        let name = property_name(&key);
        if let Some(tag) = schemas.get(&key).or_else(|| schemas.get(name)) {
            collect_f32(doc, schemas, child, &tag.data, name, &mut marked);
        }
    }
    for id in marked {
        let Some(Value::Float { v, .. }) = doc.get(id).map(|node| node.value.clone()) else {
            continue;
        };
        doc.set_value_keep_lexeme(
            id,
            Value::Float {
                v: f64::from(v as f32),
                width: FloatWidth::F32,
            },
        )?;
    }
    Ok(())
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
        mark_float32(&mut decoded.doc, &save.schemas)?;
        details::lock(&mut decoded.doc)?;
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

#[cfg(test)]
mod tests {
    use saveprism_core::edit::build_doc;
    use saveprism_core::{FloatWidth, Limits, OwnedValue, Value};
    use uesave::{PropertySchemas, PropertyTagDataPartial, PropertyTagPartial, PropertyType};

    use super::mark_float32;

    fn tag(data: PropertyTagDataPartial) -> PropertyTagPartial {
        PropertyTagPartial { id: None, data }
    }

    #[test]
    fn float_properties_get_the_32_bit_width_and_narrow_their_value() {
        let mut schemas = PropertySchemas::new();
        schemas.record(
            "f_property".to_string(),
            tag(PropertyTagDataPartial::Other(PropertyType::FloatProperty)),
        );
        schemas.record(
            "d_property".to_string(),
            tag(PropertyTagDataPartial::Other(PropertyType::DoubleProperty)),
        );
        schemas.record(
            "floats".to_string(),
            tag(PropertyTagDataPartial::Array(Box::new(
                PropertyTagDataPartial::Other(PropertyType::FloatProperty),
            ))),
        );

        let value = OwnedValue::Object(vec![(
            "root".to_string(),
            OwnedValue::Object(vec![(
                "properties".to_string(),
                OwnedValue::Object(vec![
                    ("f_property_0".to_string(), OwnedValue::Float(0.1)),
                    ("d_property_0".to_string(), OwnedValue::Float(0.1)),
                    (
                        "floats_0".to_string(),
                        OwnedValue::Array(vec![OwnedValue::Float(0.1), OwnedValue::Float(2.5)]),
                    ),
                ]),
            )]),
        )]);
        let mut doc = build_doc(&value, &Limits::default()).unwrap();
        mark_float32(&mut doc, &schemas).unwrap();

        let float = doc
            .resolve(&saveprism_core::Path::parse("/root/properties/f_property_0").unwrap())
            .unwrap();
        assert_eq!(
            doc.get(float).unwrap().value,
            Value::Float {
                v: f64::from(0.1_f32),
                width: FloatWidth::F32
            }
        );
        let double = doc
            .resolve(&saveprism_core::Path::parse("/root/properties/d_property_0").unwrap())
            .unwrap();
        assert_eq!(
            doc.get(double).unwrap().value,
            Value::Float {
                v: 0.1,
                width: FloatWidth::F64
            }
        );
        let first = doc
            .resolve(&saveprism_core::Path::parse("/root/properties/floats_0/0").unwrap())
            .unwrap();
        assert_eq!(
            doc.get(first).unwrap().value,
            Value::Float {
                v: f64::from(0.1_f32),
                width: FloatWidth::F32
            }
        );
    }
}

#[cfg(test)]
mod real_tests {
    use std::path::Path as FsPath;

    use saveprism_core::{Codec, DecodeCtx, FloatWidth, Limits, Path, Value};

    use super::GvasCodec;

    #[test]
    #[ignore = "reads the local, git-ignored fixtures/external/unreal-gvas saves"]
    fn real_options_save_round_trips_and_a_float_property_is_32_bit() {
        let path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/external/unreal-gvas/gvas-crate-tests/Options.sav");
        let Ok(input) = std::fs::read(&path) else {
            eprintln!("no real GVAS save at {}", path.display());
            return;
        };
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let decoded = GvasCodec.decode(&input, &mut ctx).unwrap();
        let id = decoded
            .doc
            .resolve(&Path::parse("/root/properties/Slider1_0").unwrap())
            .unwrap();
        assert!(matches!(
            decoded.doc.get(id).unwrap().value,
            Value::Float {
                width: FloatWidth::F32,
                ..
            }
        ));
        assert_eq!(
            GvasCodec.encode(&decoded.doc, &decoded.params).unwrap(),
            input
        );
    }
}
