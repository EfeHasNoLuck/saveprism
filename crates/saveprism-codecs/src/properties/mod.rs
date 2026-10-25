//! `format.properties`: Java `.properties` files (`key=value`, `\` escapes and continuations).
//!
//! Written from the public `java.util.Properties` text format documentation: `#`/`!` comment lines,
//! `=`, `:` or whitespace as the key/value separator, backslash escapes (`\t`, `\n`, `\r`, `\f`,
//! `\\`, `\uXXXX`) and a trailing backslash joining the next physical line.
//!
//! # Tree
//!
//! The root is a read-only `Object` (`PROPERTIES:root`) keyed by the decoded property name. A
//! repeated name is numbered in order (`name#2`). Every value is an editable `Str`
//! (`PROPERTIES:value`), decoded from its escapes.
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`. Each value records the span from its first
//! character to the end of its last physical line; writing escapes an edited value the Java way and
//! replaces only that span, so comments, keys, `:`/`=` choice and line endings stay untouched.

#[cfg(test)]
pub mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, NodeId, ParamValue,
    Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use crate::text_common::{append_numbered, node, span_range};

/// The input is not a properties file.
pub const PROPERTIES_PARSE: ErrorCode = ErrorCode::new("SP-PROPERTIES-001");
/// The document cannot be written back.
pub const PROPERTIES_WRITE: ErrorCode = ErrorCode::new("SP-PROPERTIES-002");

const PREFIX: &str = "PROPERTIES:";

/// Registers `format.properties`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(PropertiesCodec));
}

/// The `format.properties` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct PropertiesCodec;

impl Codec for PropertiesCodec {
    fn id(&self) -> &'static str {
        "format.properties"
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
        if start.starts_with('<') || start.starts_with('{') || start.starts_with('[') {
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".properties")
        });
        let other_name = ctx.file_name.is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name.ends_with(".ini") || name.ends_with(".cfg") || name.ends_with(".conf")
        });
        if other_name {
            return Confidence::NONE;
        }
        let mut pairs = 0usize;
        let mut comment = false;
        for line in text.lines() {
            let line = line.trim_start();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') || line.starts_with('!') {
                comment = true;
                continue;
            }
            if line.starts_with('[') {
                return Confidence::NONE;
            }
            if line.contains('=') || line.contains(':') {
                pairs += 1;
            }
        }
        if named && pairs > 0 {
            return Confidence::new(0.85);
        }
        if pairs >= 2 || (pairs == 1 && comment) {
            Confidence::new(0.5)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = crate::text_encoding::decode_sniff(input).ok_or_else(|| {
            Error::new(
                PROPERTIES_PARSE,
                "properties text encoding could not be detected",
            )
        })?;
        let text = decoded.text.as_str();
        let base = 0usize;
        let mut doc = ctx.new_doc();
        let mut entries: Vec<(String, NodeId)> = Vec::new();
        let mut cursor = 0usize;
        while cursor < text.len() {
            let Some((logical, next)) = logical_line(text, cursor)? else {
                break;
            };
            cursor = next;
            let LogicalLine { chars, positions } = logical;
            let mut at = 0usize;
            while chars.get(at).is_some_and(|c| c.is_whitespace()) {
                at += 1;
            }
            if at >= chars.len() || matches!(chars.get(at), Some('#' | '!')) {
                continue;
            }
            let key_start = at;
            while at < chars.len() {
                match chars.get(at) {
                    Some('\\') => at = at.saturating_add(2),
                    Some('=' | ':') => break,
                    Some(c) if c.is_whitespace() => break,
                    _ => at += 1,
                }
            }
            let key_end = at.min(chars.len());
            let key_text: String = chars
                .get(key_start..key_end)
                .unwrap_or_default()
                .iter()
                .collect();
            let key = decode_java(&key_text)?;
            while chars.get(at).is_some_and(|c| c.is_whitespace()) {
                at += 1;
            }
            if matches!(chars.get(at), Some('=' | ':')) {
                at += 1;
            }
            while chars.get(at).is_some_and(|c| c.is_whitespace()) {
                at += 1;
            }
            let value_text: String = chars.get(at..).unwrap_or_default().iter().collect();
            let value = decode_java(&value_text)?;
            let start = positions
                .get(at)
                .map(|(start, _)| base.saturating_add(*start))
                .or_else(|| positions.last().map(|(_, end)| base.saturating_add(*end)))
                .unwrap_or_else(|| base.saturating_add(cursor));
            let end = positions
                .last()
                .map(|(_, end)| base.saturating_add(*end))
                .unwrap_or(start);
            let id = doc.push(node(
                Value::Str(value),
                &format!("{PREFIX}value"),
                Some((start, end.max(start))),
                true,
            ))?;
            append_numbered(&mut doc, &mut entries, &format!("{PREFIX}values"), key, id)?;
        }
        let mut root = Node::new(Value::Object(entries));
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
            return Err(Error::new(
                PROPERTIES_WRITE,
                "the original properties text is missing",
            ));
        };
        let decoded = crate::text_common::decode_original(original, PROPERTIES_WRITE)?;
        let text = decoded.text.as_bytes();
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(PROPERTIES_WRITE, "cyclic properties tree"));
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
                .is_some_and(|hint| hint.as_str() == "PROPERTIES:value");
            let Value::Str(value) = &node.value else {
                continue;
            };
            if !is_value {
                continue;
            }
            let rendered = encode_java(value);
            let (start, end) = span_range(node, PROPERTIES_WRITE)?;
            let old = text
                .get(start..end)
                .ok_or_else(|| Error::new(PROPERTIES_WRITE, "value span outside the original"))?;
            if old != rendered.as_bytes() {
                patches.push((start, end, rendered.into_bytes()));
            }
        }
        crate::text_common::apply_encoded_patches(
            original,
            &decoded,
            PROPERTIES_WRITE,
            &mut patches,
        )
    }
}

struct LogicalLine {
    chars: Vec<char>,
    positions: Vec<(usize, usize)>,
}

/// Reads one logical line (joining backslash continuations) starting at `cursor`.
fn logical_line(text: &str, cursor: usize) -> Result<Option<(LogicalLine, usize)>> {
    if cursor >= text.len() {
        return Ok(None);
    }
    let mut chars = Vec::new();
    let mut positions = Vec::new();
    let mut at = cursor;
    while at < text.len() {
        let rest = text.get(at..).unwrap_or_default();
        let Some(character) = rest.chars().next() else {
            break;
        };
        let width = character.len_utf8();
        if character == '\\' {
            let after = at.saturating_add(1);
            let next = text.get(after..).unwrap_or_default().chars().next();
            if next == Some('\r') || next == Some('\n') {
                let mut skip = after;
                if text
                    .get(skip..)
                    .is_some_and(|tail| tail.starts_with("\r\n"))
                {
                    skip += 2;
                } else if next.is_some() {
                    skip += 1;
                }
                while text
                    .get(skip..)
                    .and_then(|tail| tail.chars().next())
                    .is_some_and(|c| matches!(c, ' ' | '\t' | '\u{0c}'))
                {
                    skip += 1;
                }
                at = skip;
                continue;
            }
            chars.push('\\');
            positions.push((at, at + 1));
            at += 1;
            if let Some(next) = next {
                chars.push(next);
                positions.push((at, at + next.len_utf8()));
                at += next.len_utf8();
            }
            continue;
        }
        if character == '\n' || character == '\r' {
            let next_index = if character == '\r'
                && text
                    .get(at + 1..)
                    .is_some_and(|tail| tail.starts_with('\n'))
            {
                at + 2
            } else {
                at + width
            };
            return Ok(Some((LogicalLine { chars, positions }, next_index)));
        }
        chars.push(character);
        positions.push((at, at + width));
        at += width;
    }
    Ok(Some((LogicalLine { chars, positions }, at)))
}

/// Decodes the Java escapes of a key or value.
fn decode_java(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        let Some(escape) = chars.next() else {
            return Err(Error::new(
                PROPERTIES_PARSE,
                "a properties line ends with a lone backslash",
            ));
        };
        match escape {
            't' => out.push('\t'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            'f' => out.push('\u{0c}'),
            'u' => {
                let mut digits = String::new();
                for _ in 0..4 {
                    let Some(digit) = chars.next() else {
                        return Err(Error::new(
                            PROPERTIES_PARSE,
                            "truncated \\u escape in properties",
                        ));
                    };
                    digits.push(digit);
                }
                let code = u32::from_str_radix(&digits, 16).map_err(|_| {
                    Error::new(PROPERTIES_PARSE, "invalid \\u escape in properties")
                })?;
                let decoded = char::from_u32(code).ok_or_else(|| {
                    Error::new(PROPERTIES_PARSE, "invalid \\u code point in properties")
                })?;
                out.push(decoded);
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// Escapes a value the Java way (leading spaces must be escaped).
fn encode_java(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut leading = true;
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0c}' => out.push_str("\\f"),
            ' ' if leading => out.push_str("\\ "),
            character if character < ' ' => {
                out.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => {
                out.push(character);
                leading = false;
            }
        }
        if !matches!(character, ' ' if leading) {
            leading = false;
        }
    }
    out
}
