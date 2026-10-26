//! Reads ONScripter global files and save-slot headers into the universal tree.

use saveprism_core::{
    ByteSpan, DecodeCtx, IntWidth, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint,
    Value, limits::len_u64,
};

use crate::text_encoding::TextEncoding;

use super::{GLOBAL_BASE, HINT_PREFIX, P_ORIGINAL, SAVE_MAGIC, parse_error};

pub(crate) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    if input.starts_with(SAVE_MAGIC) && input.len() >= 5 {
        read_slot(input, ctx)
    } else if global_plausible(input) {
        read_global(input, ctx)
    } else {
        Err(parse_error(
            "not an NScripter / ONScripter global file or save slot",
        ))
    }
}

pub(crate) fn global_plausible(input: &[u8]) -> bool {
    if input.len() < 5 || input.starts_with(SAVE_MAGIC) {
        return false;
    }
    let mut pos = 0usize;
    let mut count = 0usize;
    while pos < input.len() {
        let Some(number_end) = pos.checked_add(4) else {
            return false;
        };
        if number_end > input.len() {
            return false;
        }
        pos = number_end;
        let Some(rest) = input.get(pos..) else {
            return false;
        };
        let Some(nul) = rest.iter().position(|byte| *byte == 0) else {
            return false;
        };
        let text = rest.get(..nul).unwrap_or_default();
        if TextEncoding::ShiftJis.decode(text).is_none() {
            return false;
        }
        pos = pos.saturating_add(nul).saturating_add(1);
        count = count.saturating_add(1);
    }
    count > 0
}

fn read_global(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let mut doc = ctx.new_doc();
    let mut variables = Vec::new();
    let mut pos = 0usize;
    let mut index = 0usize;
    while pos < input.len() {
        let number_start = pos;
        let number_end = number_start
            .checked_add(4)
            .filter(|end| *end <= input.len())
            .ok_or_else(|| parse_error("a global variable number is truncated"))?;
        let number_bytes = input
            .get(number_start..number_end)
            .ok_or_else(|| parse_error("a global variable number is truncated"))?;
        let number_array = <[u8; 4]>::try_from(number_bytes)
            .map_err(|_| parse_error("a global variable number is truncated"))?;
        let number = i32::from_le_bytes(number_array);
        let string_start = number_end;
        let rest = input
            .get(string_start..)
            .ok_or_else(|| parse_error("a global variable string is truncated"))?;
        let nul = rest
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| parse_error("a global variable string is not terminated"))?;
        let string_end = string_start
            .checked_add(nul)
            .ok_or_else(|| parse_error("a global variable string is too long"))?;
        let text_bytes = input
            .get(string_start..string_end)
            .ok_or_else(|| parse_error("a global variable string is truncated"))?;

        let index_id = {
            let mut node = Node::new(Value::Int {
                v: i128::from(
                    i32::try_from(index)
                        .unwrap_or(i32::MAX)
                        .saturating_add(GLOBAL_BASE),
                ),
                width: IntWidth::I32,
            });
            node.type_hint = Some(hint("index"));
            node.flags.insert(NodeFlags::READ_ONLY);
            doc.push(node)?
        };
        let number_id = {
            let mut node = Node::new(Value::Int {
                v: i128::from(number),
                width: IntWidth::I32,
            });
            node.type_hint = Some(hint("number"));
            node.span = Some(ByteSpan {
                start: len_u64(number_start),
                end: len_u64(number_end),
            });
            doc.push(node)?
        };
        let string_id = match TextEncoding::ShiftJis.decode(text_bytes) {
            Some(text) => {
                let mut node = Node::new(Value::Str(text));
                node.type_hint = Some(hint("global-string"));
                node.span = Some(ByteSpan {
                    start: len_u64(string_start),
                    end: len_u64(string_end),
                });
                doc.push(node)?
            }
            None => {
                let mut node = Node::new(Value::Bytes(text_bytes.to_vec()));
                node.type_hint = Some(hint("raw-string"));
                node.span = Some(ByteSpan {
                    start: len_u64(string_start),
                    end: len_u64(string_end),
                });
                node.flags.insert(NodeFlags::READ_ONLY);
                doc.push(node)?
            }
        };
        let entries = vec![
            ("index".to_owned(), index_id),
            ("number".to_owned(), number_id),
            ("string".to_owned(), string_id),
        ];
        variables.push(push_object(&mut doc, "variable", entries, true)?);
        pos = string_end.saturating_add(1);
        index = index.saturating_add(1);
    }

    let mut node = Node::new(Value::Array(variables));
    node.type_hint = Some(hint("variables"));
    node.flags.insert(NodeFlags::READ_ONLY);
    let variables_id = doc.push(node)?;
    let root = push_object(
        &mut doc,
        "global",
        vec![("variables".to_owned(), variables_id)],
        true,
    )?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    Ok((doc, params))
}

fn read_slot(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let major = input
        .get(3)
        .copied()
        .ok_or_else(|| parse_error("the ONScripter save version is truncated"))?;
    let minor = input
        .get(4)
        .copied()
        .ok_or_else(|| parse_error("the ONScripter save version is truncated"))?;

    let (description_range, body_end) = trailing_description(input);
    let mut doc = ctx.new_doc();
    let magic_id = {
        let mut node = Node::new(Value::Str(
            std::str::from_utf8(SAVE_MAGIC)
                .map_err(|_| parse_error("the ONScripter magic is not text"))?
                .to_owned(),
        ));
        node.type_hint = Some(hint("magic"));
        node.flags.insert(NodeFlags::READ_ONLY);
        doc.push(node)?
    };
    let major_id = readonly_int(&mut doc, i128::from(major), IntWidth::U8, "version-major")?;
    let minor_id = readonly_int(&mut doc, i128::from(minor), IntWidth::U8, "version-minor")?;
    let version_id = readonly_int(
        &mut doc,
        i128::from(major) * 100 + i128::from(minor),
        IntWidth::I32,
        "file-version",
    )?;

    let mut entries = vec![
        ("magic".to_owned(), magic_id),
        ("version_major".to_owned(), major_id),
        ("version_minor".to_owned(), minor_id),
        ("file_version".to_owned(), version_id),
    ];
    if let Some((start, end)) = description_range {
        let text_bytes = input
            .get(start..end)
            .ok_or_else(|| parse_error("the ONScripter description is truncated"))?;
        let node_id = match TextEncoding::ShiftJis.decode(text_bytes) {
            Some(text) => {
                let mut node = Node::new(Value::Str(text));
                node.type_hint = Some(hint("save-description"));
                node.span = Some(ByteSpan {
                    start: len_u64(start),
                    end: len_u64(end),
                });
                doc.push(node)?
            }
            None => {
                let mut node = Node::new(Value::Bytes(text_bytes.to_vec()));
                node.type_hint = Some(hint("raw-description"));
                node.span = Some(ByteSpan {
                    start: len_u64(start),
                    end: len_u64(end),
                });
                node.flags.insert(NodeFlags::READ_ONLY);
                doc.push(node)?
            }
        };
        entries.push(("description".to_owned(), node_id));
    }
    let body = input
        .get(SAVE_MAGIC.len()..body_end)
        .ok_or_else(|| parse_error("the ONScripter save body is truncated"))?
        .to_vec();
    let mut body_node = Node::new(Value::Bytes(body));
    body_node.type_hint = Some(hint("body"));
    body_node.flags.insert(NodeFlags::READ_ONLY);
    let body_id = doc.push(body_node)?;
    entries.push(("body".to_owned(), body_id));
    let root = push_object(&mut doc, "save", entries, true)?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(P_ORIGINAL, input.to_vec());
    Ok((doc, params))
}

/// Finds the `"text"*` suffix ONScripter appends to a save slot.
fn trailing_description(input: &[u8]) -> (Option<(usize, usize)>, usize) {
    if input.last().copied() != Some(b'*') {
        return (None, input.len());
    }
    let quote_end = input.len().saturating_sub(2);
    if input.get(quote_end).copied() != Some(b'"') {
        return (None, input.len());
    }
    let prefix = input.get(..quote_end).unwrap_or_default();
    let Some(open) = prefix.iter().rposition(|byte| *byte == b'"') else {
        return (None, input.len());
    };
    let start = open.saturating_add(1);
    (Some((start, quote_end)), open)
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

fn readonly_int(doc: &mut SaveDoc, value: i128, width: IntWidth, name: &str) -> Result<NodeId> {
    let mut node = Node::new(Value::Int { v: value, width });
    node.type_hint = Some(hint(name));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_object(
    doc: &mut SaveDoc,
    name: &str,
    entries: Vec<(String, NodeId)>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Object(entries));
    node.type_hint = Some(hint(name));
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}
