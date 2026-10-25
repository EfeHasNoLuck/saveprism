//! `format.toml`: TOML save/config files.
//!
//! Format-preserving parsing uses `toml_edit` (MIT OR Apache-2.0), which keeps comments and
//! layout; the tree is built from its values and their byte spans. Strings, integers, floats and
//! booleans are editable; date/time values and all containers are read-only, so an edit never
//! changes the document shape.
//!
//! # Tree
//!
//! The root is a read-only `Object` (`TOML:root`). Tables and inline tables become read-only
//! `Object`s (`TOML:table`), arrays and arrays of tables read-only `Array`s (`TOML:array`).
//! Editable leaves carry `TOML:string`, `TOML:int`, `TOML:float` or `TOML:bool`; a date/time is a
//! read-only `TOML:datetime`.
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`; writing replaces only the spans of edited
//! leaves, in canonical TOML syntax, and copies comments, spacing and line endings unchanged.

#[cfg(test)]
pub mod tests;

use std::str::FromStr;
use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, FloatWidth, IntWidth, Node, NodeFlags,
    ParamValue, Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};
use toml_edit::{Document, Item, Table, Value as TomlValue};

use crate::text_common::{format_float, node, span_range};

/// The input is not valid TOML.
pub const TOML_PARSE: ErrorCode = ErrorCode::new("SP-TOML-001");
/// The document cannot be written back.
pub const TOML_WRITE: ErrorCode = ErrorCode::new("SP-TOML-002");

const PREFIX: &str = "TOML:";

/// Registers `format.toml`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(TomlCodec));
}

/// The `format.toml` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct TomlCodec;

impl Codec for TomlCodec {
    fn id(&self) -> &'static str {
        "format.toml"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        if ctx
            .file_name
            .is_some_and(crate::text_common::is_generic_text_name)
        {
            return Confidence::NONE;
        }
        if Document::<String>::from_str(text).is_err() {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".toml"));
        // Godot ConfigFile and INI are also sectioned key/value text; keep below them so they win
        // their own syntax. A `.toml` name is strong evidence.
        if named {
            Confidence::new(0.85)
        } else {
            Confidence::new(0.55)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let text = std::str::from_utf8(input)
            .map_err(|_| Error::new(TOML_PARSE, "TOML is not valid UTF-8"))?;
        let document = Document::<String>::from_str(text)
            .map_err(|error| Error::new(TOML_PARSE, format!("invalid TOML: {error}")))?;
        let mut doc = ctx.new_doc();
        let mut entries = Vec::new();
        for (key, item) in document.as_table().iter() {
            let id = build_item(&mut doc, item, ctx.limits().max_depth, 0)?;
            entries.push((key.to_owned(), id));
        }
        let mut root = Node::new(Value::Object(entries));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}root")));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(TOML_WRITE, "the original TOML text is missing"));
        };
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(TOML_WRITE, "cyclic TOML tree"));
            }
            let Some(node) = doc.get(id) else {
                continue;
            };
            stack.extend(doc.children(id));
            if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
                continue;
            }
            let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
                continue;
            };
            let rendered = match (hint, &node.value) {
                ("TOML:string", Value::Str(text)) => escape_basic(text),
                ("TOML:int", Value::Int { v, .. }) => v.to_string(),
                ("TOML:float", Value::Float { v, .. }) => toml_float(*v),
                ("TOML:bool", Value::Bool(value)) => value.to_string(),
                _ => continue,
            };
            let (start, end) = span_range(node, TOML_WRITE)?;
            let old = original
                .get(start..end)
                .ok_or_else(|| Error::new(TOML_WRITE, "value span outside the original text"))?;
            if old != rendered.as_bytes() {
                patches.push((start, end, rendered.into_bytes()));
            }
        }
        crate::text_common::apply_patches(original, TOML_WRITE, &mut patches)
    }
}

fn check_depth(max_depth: usize, depth: usize) -> Result<()> {
    if depth > max_depth {
        return Err(Error::new(
            saveprism_core::codes::CORE_LIMIT_DEPTH,
            format!("TOML nesting limit of {max_depth} exceeded"),
        ));
    }
    Ok(())
}

fn build_item(
    doc: &mut SaveDoc,
    item: &Item,
    max_depth: usize,
    depth: usize,
) -> Result<saveprism_core::NodeId> {
    check_depth(max_depth, depth)?;
    match item {
        Item::Value(value) => build_value(doc, value, max_depth, depth),
        Item::Table(table) => build_table(doc, table, max_depth, depth),
        Item::ArrayOfTables(tables) => {
            let mut items = Vec::new();
            for table in tables.iter() {
                items.push(build_table(doc, table, max_depth, depth + 1)?);
            }
            let id = doc.push(crate::text_common::read_only(
                Value::Array(items),
                &format!("{PREFIX}array"),
            ))?;
            Ok(id)
        }
        Item::None => Ok(doc.push(crate::text_common::read_only(
            Value::Null,
            &format!("{PREFIX}null"),
        ))?),
    }
}

fn build_table(
    doc: &mut SaveDoc,
    table: &Table,
    max_depth: usize,
    depth: usize,
) -> Result<saveprism_core::NodeId> {
    let mut entries = Vec::new();
    for (key, item) in table.iter() {
        entries.push((key.to_owned(), build_item(doc, item, max_depth, depth + 1)?));
    }
    let id = doc.push(crate::text_common::read_only(
        Value::Object(entries),
        &format!("{PREFIX}table"),
    ))?;
    Ok(id)
}

fn build_value(
    doc: &mut SaveDoc,
    value: &TomlValue,
    max_depth: usize,
    depth: usize,
) -> Result<saveprism_core::NodeId> {
    check_depth(max_depth, depth)?;
    let span = value.span().map(|range| (range.start, range.end));
    let editable =
        |doc: &mut SaveDoc, value: Value, hint: &str| -> Result<saveprism_core::NodeId> {
            doc.push(node(value, &format!("{PREFIX}{hint}"), span, true))
        };
    match value {
        TomlValue::String(text) => editable(doc, Value::Str(text.value().to_owned()), "string"),
        TomlValue::Integer(number) => editable(
            doc,
            Value::Int {
                v: i128::from(*number.value()),
                width: IntWidth::I64,
            },
            "int",
        ),
        TomlValue::Float(number) => editable(
            doc,
            Value::Float {
                v: *number.value(),
                width: FloatWidth::F64,
            },
            "float",
        ),
        TomlValue::Boolean(flag) => editable(doc, Value::Bool(*flag.value()), "bool"),
        TomlValue::Datetime(datetime) => Ok(doc.push(node(
            Value::Str(datetime.to_string()),
            &format!("{PREFIX}datetime"),
            span,
            false,
        ))?),
        TomlValue::Array(array) => {
            let mut items = Vec::new();
            for item in array.iter() {
                items.push(build_value(doc, item, max_depth, depth + 1)?);
            }
            Ok(doc.push(crate::text_common::read_only(
                Value::Array(items),
                &format!("{PREFIX}array"),
            ))?)
        }
        TomlValue::InlineTable(table) => {
            let mut entries = Vec::new();
            for (key, item) in table.iter() {
                entries.push((
                    key.to_owned(),
                    build_value(doc, item, max_depth, depth + 1)?,
                ));
            }
            Ok(doc.push(crate::text_common::read_only(
                Value::Object(entries),
                &format!("{PREFIX}table"),
            ))?)
        }
    }
}

fn toml_float(value: f64) -> String {
    if value.is_nan() {
        "nan".to_owned()
    } else if value == f64::INFINITY {
        "inf".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_owned()
    } else {
        format_float(value)
    }
}

/// A TOML basic string with the escapes of the public TOML 1.0 specification.
fn escape_basic(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            character if character < ' ' || character == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}
