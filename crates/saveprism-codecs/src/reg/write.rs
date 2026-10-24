//! Writes a `.reg` document: original chunks verbatim, edited values rendered again.

use saveprism_core::{Error, Node, Result, SaveDoc, Value};

use super::read::{ParsedFile, ValueData, parse};
use super::{HEADER_V5, REG_WRITE, TextEncoding, encode_text, parse_value_hint};

/// Wraps long hex values at about the width regedit uses.
const MAX_LINE: usize = 80;

fn write_error(message: impl Into<String>) -> Error {
    Error::new(REG_WRITE, message)
}

/// Writes the whole document using the original bytes for every unchanged part.
pub(crate) fn encode_original(
    doc: &SaveDoc,
    original: &[u8],
    encoding: TextEncoding,
    bom: bool,
) -> Result<Vec<u8>> {
    let parsed = parse(original)?;
    let values = flatten(doc, &parsed)?;
    let mut out = String::new();
    let mut next = 0_usize;
    for chunk in &parsed.chunks {
        match chunk {
            super::read::Chunk::Raw(text) => out.push_str(text),
            super::read::Chunk::Value { section, index } => {
                let Some(node) = values.get(next) else {
                    return Err(write_error("the document has fewer values than the file"));
                };
                next += 1;
                let Some(data) = parsed
                    .sections
                    .get(*section)
                    .and_then(|section| section.values.get(*index))
                else {
                    return Err(write_error("the parsed file lost a value"));
                };
                if same_value(&node.value, &data.value) {
                    out.push_str(&data.raw);
                } else {
                    render(&mut out, data, node)?;
                }
            }
        }
    }
    if next != values.len() {
        return Err(write_error("the document has more values than the file"));
    }
    encode_text(&out, encoding, bom)
}

/// Current value nodes in section/value order.
fn flatten<'d>(doc: &'d SaveDoc, parsed: &ParsedFile) -> Result<Vec<&'d Node>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| write_error("the document has no root"))?;
    let Value::Object(sections) = &root.value else {
        return Err(write_error("the document root is not an object of keys"));
    };
    if sections.len() != parsed.sections.len() {
        return Err(write_error(
            "the document has a different number of keys than the file",
        ));
    }
    let mut values = Vec::new();
    for (_, key_id) in sections {
        let key = doc
            .get(*key_id)
            .ok_or_else(|| write_error("a key node is missing"))?;
        let Value::Object(entries) = &key.value else {
            return Err(write_error("a key is not an object of values"));
        };
        for (_, value_id) in entries {
            let node = doc
                .get(*value_id)
                .ok_or_else(|| write_error("a value node is missing"))?;
            values.push(node);
        }
    }
    Ok(values)
}

/// Compares without `-0.0 == 0.0` hiding an edit.
fn same_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int { v: a, .. }, Value::Int { v: b, .. }) => a == b,
        (Value::Float { v: a, .. }, Value::Float { v: b, .. }) => a.to_bits() == b.to_bits(),
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bytes(a), Value::Bytes(b)) => a == b,
        _ => false,
    }
}

fn hint_parts(node: &Node) -> Option<(&str, &str)> {
    parse_value_hint(node.type_hint.as_ref()?.as_str())
}

/// Renders one edited value with the line ending of its first original line.
fn render(out: &mut String, data: &ValueData, node: &Node) -> Result<()> {
    let eol = if data.eol.is_empty() {
        "\r\n"
    } else {
        &data.eol
    };
    render_value(
        out,
        &data.full_name,
        data_hint_tag(data),
        &node.value,
        eol,
        false,
    )
}

fn data_hint_tag(data: &ValueData) -> &str {
    parse_value_hint(&data.hint).map_or("", |(tag, _)| tag)
}

/// Renders one value line (or a wrapped hex block) into `out`.
fn render_value(
    out: &mut String,
    full_name: &str,
    tag: &str,
    value: &Value,
    eol: &str,
    fallback: bool,
) -> Result<()> {
    let name = if full_name == "@" {
        "@".to_string()
    } else {
        format!("\"{}\"", escape_name(full_name))
    };
    match tag {
        "sz" => {
            let Value::Str(text) = value else {
                return Err(write_error("a REG_SZ value is no longer text"));
            };
            out.push_str(&name);
            out.push_str("=\"");
            out.push_str(&escape_string(text));
            out.push('"');
            out.push_str(eol);
        }
        "dword" | "hex4-int" => {
            let Value::Int { v, .. } = value else {
                return Err(write_error("a dword value is no longer an integer"));
            };
            let unsigned = u32::try_from(*v)
                .or_else(|_| i32::try_from(*v).map(|signed| signed as u32))
                .map_err(|_| write_error("the dword value is outside the 32-bit range"))?;
            out.push_str(&name);
            if tag == "hex4-int" {
                out.push_str("=hex(4):");
                out.push_str(&hex_text(&unsigned.to_le_bytes()));
            } else {
                out.push_str(&format!("=dword:{unsigned:08x}"));
            }
            out.push_str(eol);
        }
        "qword" | "hex4" => {
            let Value::Float { v, .. } = value else {
                return Err(write_error("a qword value is no longer a float"));
            };
            let bytes = v.to_bits().to_le_bytes();
            let token = if tag == "hex4" { "hex(4):" } else { "hex(b):" };
            out.push_str(&name);
            out.push('=');
            push_hex(out, token, &bytes, eol);
        }
        "unity-string" => {
            let Value::Str(text) = value else {
                return Err(write_error("a Unity string is no longer text"));
            };
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0x00);
            out.push_str(&name);
            push_hex(out, "=hex:", &bytes, eol);
        }
        "binary" | "expand" | "multi" | "other" | "delete" => {
            out.push_str(&name);
            match (tag, value) {
                ("delete", Value::Null) => out.push('='),
                (_, Value::Bytes(bytes)) => {
                    let token = match tag {
                        "expand" => "=hex(2):",
                        "multi" => "=hex(7):",
                        _ => "=hex:",
                    };
                    push_hex(out, token, bytes, eol);
                    return Ok(());
                }
                _ => {
                    return Err(write_error("a read-only value changed its type"));
                }
            }
            out.push('-');
            out.push_str(eol);
        }
        _ if fallback => {
            render_derived(out, full_name, value, eol)?;
        }
        _ => {
            return Err(write_error(format!(
                "unknown format.reg value kind \"{tag}\""
            )));
        }
    }
    Ok(())
}

/// The canonical whole-file writer, used when no original bytes are in the parameters.
pub(crate) fn encode_canonical(doc: &SaveDoc) -> Result<Vec<u8>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| write_error("the document has no root"))?;
    let Value::Object(sections) = &root.value else {
        return Err(write_error("the document root is not an object of keys"));
    };
    let mut out = String::new();
    out.push_str(HEADER_V5);
    out.push_str("\r\n\r\n");
    for (key_name, key_id) in sections {
        let key = doc
            .get(*key_id)
            .ok_or_else(|| write_error("a key node is missing"))?;
        let Value::Object(entries) = &key.value else {
            return Err(write_error("a key is not an object of values"));
        };
        out.push('[');
        out.push_str(key_name);
        out.push_str("]\r\n");
        for (entry_name, value_id) in entries {
            let node = doc
                .get(*value_id)
                .ok_or_else(|| write_error("a value node is missing"))?;
            let (tag, full_name) = match hint_parts(node) {
                Some(parts) => parts,
                None => (derived_tag(&node.value), entry_name.as_str()),
            };
            render_value(&mut out, full_name, tag, &node.value, "\r\n", true)?;
        }
        out.push_str("\r\n");
    }
    encode_text(&out, TextEncoding::Utf16Le, true)
}

fn derived_tag(value: &Value) -> &'static str {
    match value {
        Value::Int { .. } => "dword",
        Value::Float { .. } => "qword",
        Value::Str(_) => "sz",
        Value::Bytes(_) => "binary",
        Value::Null => "delete",
        _ => "",
    }
}

fn render_derived(out: &mut String, full_name: &str, value: &Value, eol: &str) -> Result<()> {
    let tag = derived_tag(value);
    if tag.is_empty() {
        return Err(write_error(
            "the document holds a value a .reg file cannot write",
        ));
    }
    render_value(out, full_name, tag, value, eol, false)
}

fn escape_name(name: &str) -> String {
    name.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_string(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn hex_text(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Appends `prefix` (already including the `=` and type) then the bytes, wrapping long values
/// with `,\` and a two-space indent.
fn push_hex(out: &mut String, prefix: &str, bytes: &[u8], eol: &str) {
    let mut line = String::from(prefix);
    let mut width = prefix.chars().count();
    for (index, byte) in bytes.iter().enumerate() {
        let last = index + 1 == bytes.len();
        let mut cell = format!("{byte:02x}");
        if !last {
            cell.push(',');
        }
        let cell_width = cell.chars().count();
        if width > 2 && width + cell_width + 1 > MAX_LINE {
            line.push('\\');
            out.push_str(&line);
            out.push_str(eol);
            line = String::from("  ");
            width = 2;
        }
        line.push_str(&cell);
        width += cell_width;
    }
    line.push_str(eol);
    out.push_str(&line);
}
