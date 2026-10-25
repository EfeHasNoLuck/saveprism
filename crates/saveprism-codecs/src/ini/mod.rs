//! `format.ini`: INI/CFG-style files (`[section]` and `key = value`, `;` or `#` comments).
//!
//! Written from the widely used INI conventions (sections, `=` assignments, `;` and `#` comment
//! lines), not from any closed implementation.
//!
//! # Tree
//!
//! The root is a read-only `Object` (`INI:root`) of sections; the keys of the unnamed leading
//! section live under the empty key `""`. A section is a read-only `Object` (`INI:section`) of its
//! key/value pairs. A repeated key or section is numbered in order (`level#2`, `[Audio]` then
//! `Audio#2`). Every value is an editable `Str` (`INI:value`).
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`. Every value records the span of its token
//! (not the surrounding spaces or an inline comment); writing replaces only edited tokens and
//! copies every comment, blank line, section header, key and line ending unchanged.

#[cfg(test)]
pub mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, NodeId, ParamValue,
    Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use crate::text_common::{append_numbered, node, span_range};

/// The input is not INI-like.
pub const INI_PARSE: ErrorCode = ErrorCode::new("SP-INI-001");
/// The document cannot be written back.
pub const INI_WRITE: ErrorCode = ErrorCode::new("SP-INI-002");

const PREFIX: &str = "INI:";

/// Registers `format.ini`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(IniCodec));
}

/// The `format.ini` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct IniCodec;

impl Codec for IniCodec {
    fn id(&self) -> &'static str {
        "format.ini"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Some(decoded) = crate::text_encoding::decode_sniff(input) else {
            return Confidence::NONE;
        };
        let text = decoded.text.as_str();
        if ctx
            .file_name
            .is_some_and(crate::text_common::is_generic_text_name)
        {
            return Confidence::NONE;
        }
        let start = text.trim_start();
        if start.starts_with('<') || start.starts_with('{') || start.starts_with("[{") {
            return Confidence::NONE;
        }
        let mut section = false;
        let mut assignment = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && !line.contains('=') {
                section = true;
            } else if let Some((key, _)) = line.split_once('=') {
                let key = key.trim();
                if !key.is_empty() && !key.contains(['[', ']', '"', '{', '}']) {
                    assignment = true;
                }
            }
        }
        let named = ctx.file_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".ini") || name.ends_with(".cfg") || name.ends_with(".conf")
        });
        // Deliberately below `format.godot-config`: a Godot ConfigFile keeps winning on its syntax.
        let score = match (section, assignment, named) {
            (true, true, true) => 0.78,
            (true, true, false) => 0.7,
            (false, true, true) => 0.5,
            _ => return Confidence::NONE,
        };
        Confidence::new(score)
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = crate::text_encoding::decode_sniff(input)
            .ok_or_else(|| Error::new(INI_PARSE, "INI text encoding could not be detected"))?;
        let text = decoded.text.as_str();
        let base = 0usize;
        let mut doc = ctx.new_doc();
        let mut sections: Vec<(String, NodeId)> = Vec::new();
        let mut section_name = String::new();
        let mut entries: Vec<(String, NodeId)> = Vec::new();
        let mut have_section = false;
        let mut offset = 0usize;
        for line in text.split_inclusive('\n') {
            let content = line.strip_suffix('\n').unwrap_or(line);
            let content = content.strip_suffix('\r').unwrap_or(content);
            let trimmed = content.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
                offset = offset.saturating_add(line.len());
                continue;
            }
            if let Some(inner) = trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                if have_section || !entries.is_empty() {
                    flush_section(&mut doc, &mut sections, &mut section_name, &mut entries)?;
                }
                section_name = inner.trim().to_owned();
                have_section = true;
                offset = offset.saturating_add(line.len());
                continue;
            }
            let Some(equals) = line.find('=') else {
                return Err(Error::new(
                    INI_PARSE,
                    format!("expected a section or key=value at byte {offset}"),
                ));
            };
            let key = line.get(..equals).unwrap_or_default().trim().to_owned();
            if key.is_empty() {
                return Err(Error::new(INI_PARSE, format!("empty key at byte {offset}")));
            }
            let raw = line.get(equals + 1..).unwrap_or_default();
            let comment = inline_comment(raw);
            let value_raw = raw.get(..comment).unwrap_or(raw);
            let leading = value_raw.len().saturating_sub(value_raw.trim_start().len());
            let trailing = value_raw.len().saturating_sub(value_raw.trim_end().len());
            let end_offset = value_raw.len().saturating_sub(trailing);
            let start_offset = leading.min(end_offset);
            let start = base
                .saturating_add(offset)
                .saturating_add(equals)
                .saturating_add(1)
                .saturating_add(start_offset);
            let end = base
                .saturating_add(offset)
                .saturating_add(equals)
                .saturating_add(1)
                .saturating_add(end_offset);
            let value = value_raw.get(start_offset..end_offset).unwrap_or_default();
            let id = doc.push(node(
                Value::Str(value.to_owned()),
                &format!("{PREFIX}value"),
                Some((start, end)),
                true,
            ))?;
            append_numbered(&mut doc, &mut entries, &format!("{PREFIX}values"), key, id)?;
            offset = offset.saturating_add(line.len());
        }
        if have_section || !entries.is_empty() {
            flush_section(&mut doc, &mut sections, &mut section_name, &mut entries)?;
        }
        let mut root = Node::new(Value::Object(sections));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}root")));
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
            return Err(Error::new(INI_WRITE, "the original INI text is missing"));
        };
        let decoded = crate::text_common::decode_original(original, INI_WRITE)?;
        let text = decoded.text.as_bytes();
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(INI_WRITE, "cyclic INI tree"));
            }
            let Some(node) = doc.get(id) else {
                continue;
            };
            stack.extend(doc.children(id));
            if node.flags.contains(NodeFlags::READ_ONLY) || node.lexeme.is_some() {
                continue;
            }
            let is_value = node
                .type_hint
                .as_ref()
                .is_some_and(|hint| hint.as_str() == "INI:value");
            if !is_value {
                continue;
            }
            let Value::Str(value) = &node.value else {
                continue;
            };
            if value.contains(['\r', '\n']) {
                return Err(Error::new(
                    INI_WRITE,
                    "an INI value cannot contain a line break",
                ));
            }
            let (start, end) = span_range(node, INI_WRITE)?;
            let old = text
                .get(start..end)
                .ok_or_else(|| Error::new(INI_WRITE, "value span outside the original text"))?;
            if old != value.as_bytes() {
                patches.push((start, end, value.as_bytes().to_vec()));
            }
        }
        crate::text_common::apply_encoded_patches(original, &decoded, INI_WRITE, &mut patches)
    }
}

fn flush_section(
    doc: &mut SaveDoc,
    sections: &mut Vec<(String, NodeId)>,
    name: &mut String,
    entries: &mut Vec<(String, NodeId)>,
) -> Result<()> {
    let mut section = Node::new(Value::Object(std::mem::take(entries)));
    section.type_hint = Some(TypeHint::new(format!("{PREFIX}section")));
    section.flags.insert(NodeFlags::READ_ONLY);
    let id = doc.push(section)?;
    append_numbered(
        doc,
        sections,
        &format!("{PREFIX}sections"),
        std::mem::take(name),
        id,
    )
}

/// Byte position where an inline comment starts, or the length of `raw`.
fn inline_comment(raw: &str) -> usize {
    let bytes = raw.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b';' | b'#') {
            let previous = index.checked_sub(1).and_then(|at| bytes.get(at)).copied();
            if previous.is_none_or(|previous| previous.is_ascii_whitespace()) {
                return index;
            }
        }
    }
    raw.len()
}
