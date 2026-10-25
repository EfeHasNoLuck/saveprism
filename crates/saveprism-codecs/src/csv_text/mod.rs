//! `format.csv`: comma-separated save tables.
//!
//! Written from RFC 4180 (quoted fields, doubled quotes, CRLF or LF records). The parser keeps the
//! raw span of every field, so an edit changes one cell and nothing else.
//!
//! # Tree
//!
//! The root is a read-only `Array` of rows (`CSV:rows`). When the first record looks like a header
//! (non-empty, unique identifier-like names) it names the columns and is not repeated in the tree;
//! each row is then a read-only `Object` (`CSV:row`) keyed by those names. Without a header, rows
//! are read-only arrays of cells. Cells are editable `Str` nodes (`CSV:cell`); quoting is kept.
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`. A quoted cell stays quoted, an unquoted
//! cell gets quotes only when the new value needs them (comma, quote or a line break), and every
//! other byte of the row, the header and the file is copied unchanged.

#[cfg(test)]
pub mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, ParamValue, Params,
    Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use crate::text_common::{node, span_range};

/// The input is not CSV.
pub const CSV_PARSE: ErrorCode = ErrorCode::new("SP-CSV-001");
/// The document cannot be written back.
pub const CSV_WRITE: ErrorCode = ErrorCode::new("SP-CSV-002");

const PREFIX: &str = "CSV:";

/// Registers `format.csv`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(CsvCodec));
}

/// The `format.csv` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct CsvCodec;

impl Codec for CsvCodec {
    fn id(&self) -> &'static str {
        "format.csv"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(decoded) = crate::text_encoding::decode_sniff(input) else {
            return Confidence::NONE;
        };
        let body = decoded.text.as_str();
        let start = body.trim_start();
        if start.is_empty()
            || start.starts_with('<')
            || start.starts_with('{')
            || start.starts_with('[')
        {
            return Confidence::NONE;
        }
        if ctx
            .file_name
            .is_some_and(crate::text_common::is_generic_text_name)
        {
            return Confidence::NONE;
        }
        let name = ctx.file_name.map(str::to_ascii_lowercase);
        if name
            .as_deref()
            .is_some_and(|name| name.ends_with(".yaml") || name.ends_with(".yml"))
        {
            return Confidence::NONE;
        }
        let named = name.as_deref().is_some_and(|name| name.ends_with(".csv"));
        let mut rows = 0usize;
        let mut comma_lines = 0usize;
        for line in body.lines().take(32) {
            if line.trim().is_empty() {
                continue;
            }
            rows += 1;
            if line.contains(',') {
                comma_lines += 1;
            }
        }
        if named && comma_lines > 0 {
            return Confidence::new(0.8);
        }
        if comma_lines >= 2 && comma_lines.saturating_mul(2) >= rows {
            Confidence::new(0.55)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = crate::text_encoding::decode_sniff(input)
            .ok_or_else(|| Error::new(CSV_PARSE, "CSV text encoding could not be detected"))?;
        let text = decoded.text.as_str();
        let records = parse_records(text, 0)?;
        if records.is_empty() {
            return Err(Error::new(CSV_PARSE, "the CSV file has no records"));
        }
        let mut doc = ctx.new_doc();
        let header = header_names(&records);
        let (header, data) = match header {
            Some(names) => (Some(names), records.get(1..).unwrap_or_default()),
            None => (None, records.as_slice()),
        };
        let mut rows = Vec::new();
        for record in data {
            let id = match &header {
                Some(names) => {
                    let mut entries = Vec::new();
                    for (index, field) in record.iter().enumerate() {
                        let key = names
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("column{}", index + 1));
                        let cell = doc.push(cell_node(field))?;
                        entries.push((key, cell));
                    }
                    let row = Node::new(Value::Object(entries));
                    let mut row = row;
                    row.type_hint = Some(TypeHint::new(format!("{PREFIX}row")));
                    row.flags.insert(NodeFlags::READ_ONLY);
                    doc.push(row)?
                }
                None => {
                    let mut cells = Vec::new();
                    for field in record {
                        cells.push(doc.push(cell_node(field))?);
                    }
                    let mut row = Node::new(Value::Array(cells));
                    row.type_hint = Some(TypeHint::new(format!("{PREFIX}row")));
                    row.flags.insert(NodeFlags::READ_ONLY);
                    doc.push(row)?
                }
            };
            rows.push(id);
        }
        let mut root = Node::new(Value::Array(rows));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}rows")));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("text_encoding", decoded.encoding.name());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(CSV_WRITE, "the original CSV text is missing"));
        };
        let decoded = crate::text_common::decode_original(original, CSV_WRITE)?;
        let text = decoded.text.as_bytes();
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(CSV_WRITE, "cyclic CSV tree"));
            }
            let Some(node) = doc.get(id) else {
                continue;
            };
            stack.extend(doc.children(id));
            if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
                continue;
            }
            let is_cell = node
                .type_hint
                .as_ref()
                .is_some_and(|hint| hint.as_str() == "CSV:cell");
            let Value::Str(value) = &node.value else {
                continue;
            };
            if !is_cell {
                continue;
            }
            let (start, end) = span_range(node, CSV_WRITE)?;
            let old = text
                .get(start..end)
                .ok_or_else(|| Error::new(CSV_WRITE, "cell span outside the original text"))?;
            let quoted = old.first() == Some(&b'"');
            let rendered = render_cell(value, quoted);
            if old != rendered.as_bytes() {
                patches.push((start, end, rendered.into_bytes()));
            }
        }
        crate::text_common::apply_encoded_patches(original, &decoded, CSV_WRITE, &mut patches)
    }
}

struct Field {
    start: usize,
    end: usize,
    value: String,
    quoted: bool,
}

fn cell_node(field: &Field) -> Node {
    node(
        Value::Str(field.value.clone()),
        &format!("{PREFIX}cell"),
        Some((field.start, field.end)),
        true,
    )
}

fn render_cell(value: &str, quoted: bool) -> String {
    let needs = value.contains([',', '"', '\r', '\n'])
        || value.starts_with(char::is_whitespace)
        || value.ends_with(char::is_whitespace);
    if quoted || needs {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn header_names(records: &[Vec<Field>]) -> Option<Vec<String>> {
    if records.len() < 2 {
        return None;
    }
    let first = records.first()?;
    if first.len() < 2 {
        return None;
    }
    let mut names = Vec::with_capacity(first.len());
    for field in first {
        let name = field.value.trim();
        if name.is_empty()
            || !name
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '-' | '.'))
        {
            return None;
        }
        if names.iter().any(|existing: &String| existing == name) {
            return None;
        }
        names.push(name.to_owned());
    }
    Some(names)
}

fn parse_records(text: &str, base: usize) -> Result<Vec<Vec<Field>>> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut records = Vec::new();
    while i < bytes.len() {
        let mut fields = Vec::new();
        loop {
            let field_start = i;
            let mut quoted = false;
            let mut value = String::new();
            if bytes.get(i) == Some(&b'"') {
                quoted = true;
                i += 1;
                loop {
                    match bytes.get(i).copied() {
                        Some(b'"') => {
                            if bytes.get(i + 1) == Some(&b'"') {
                                value.push('"');
                                i += 2;
                            } else {
                                i += 1;
                                break;
                            }
                        }
                        Some(_) => {
                            let rest = text.get(i..).unwrap_or_default();
                            let Some(character) = rest.chars().next() else {
                                break;
                            };
                            value.push(character);
                            i += character.len_utf8();
                        }
                        None => {
                            return Err(Error::new(CSV_PARSE, "unterminated quoted CSV field"));
                        }
                    }
                }
                while bytes.get(i).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                    i += 1;
                }
            } else {
                let start = i;
                while bytes
                    .get(i)
                    .is_some_and(|b| !matches!(b, b',' | b'\r' | b'\n'))
                {
                    i += 1;
                }
                value = text.get(start..i).unwrap_or_default().to_owned();
            }
            fields.push(Field {
                start: base.saturating_add(field_start),
                end: base.saturating_add(i),
                value,
                quoted,
            });
            match bytes.get(i).copied() {
                Some(b',') => {
                    i += 1;
                }
                Some(b'\r') => {
                    i += 1;
                    if bytes.get(i) == Some(&b'\n') {
                        i += 1;
                    }
                    break;
                }
                Some(b'\n') => {
                    i += 1;
                    break;
                }
                None => break,
                _ => {
                    return Err(Error::new(
                        CSV_PARSE,
                        "unexpected byte after a quoted field",
                    ));
                }
            }
        }
        let blank = fields.len() == 1
            && fields
                .first()
                .is_some_and(|field| !field.quoted && field.value.is_empty());
        if !blank {
            records.push(fields);
        }
    }
    Ok(records)
}
