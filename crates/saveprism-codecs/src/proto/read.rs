//! The Protocol Buffers wire-format reader (see `mod.rs` for the tree and the rules).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result, SaveDoc, TypeHint,
    Value, codes,
};

use super::{HINT_PREFIX, ORIGINAL, PROTO_PARSE};

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(PROTO_PARSE, message).with_offset(len_u64(at))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

/// Reads one varint. Returns `(value, bytes_used)`; the tag and payload share this helper.
fn read_varint(input: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for index in 0..10_usize {
        let byte = *input.get(pos.checked_add(index)?)?;
        let payload = u64::from(byte & 0x7f);
        let shift = u32::try_from(index).ok()?.checked_mul(7)?;
        if shift >= 64 && payload != 0 {
            return None;
        }
        value |= payload.checked_shl(shift).unwrap_or(0);
        if byte & 0x80 == 0 {
            return Some((value, index + 1));
        }
    }
    None
}

fn encode_varint(mut value: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = u8::try_from(value & 0x7f).unwrap_or(0);
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn field_tag(number: u64, wire: u8) -> u64 {
    (number << 3) | u64::from(wire & 7)
}

/// True when `number` is a valid protobuf field number (1..2^29-1, not the reserved 19000..20000).
fn valid_field_number(number: u64) -> bool {
    (1..=0x1fff_ffff).contains(&number) && !(19000..=19999).contains(&number)
}

/// Walks a group and returns the position after its matching end tag.
fn scan_group(
    input: &[u8],
    mut pos: usize,
    end: usize,
    number: u64,
    depth: usize,
    max_depth: usize,
) -> Option<usize> {
    if depth >= max_depth {
        return None;
    }
    while pos < end {
        let (tag, used) = read_varint(input, pos)?;
        pos = pos.checked_add(used)?;
        let inner_number = tag >> 3;
        let wire = u8::try_from(tag & 7).ok()?;
        if !valid_field_number(inner_number) {
            return None;
        }
        match wire {
            0 => {
                let (_, used) = read_varint(input, pos)?;
                pos = pos.checked_add(used)?;
            }
            1 => pos = pos.checked_add(8)?,
            2 => {
                let (length, used) = read_varint(input, pos)?;
                pos = pos.checked_add(used)?;
                let length = usize::try_from(length).ok()?;
                pos = pos.checked_add(length)?;
            }
            3 => {
                pos = scan_group(input, pos, end, inner_number, depth + 1, max_depth)?;
            }
            4 => {
                if inner_number != number {
                    return None;
                }
                return (pos <= end).then_some(pos);
            }
            5 => pos = pos.checked_add(4)?,
            _ => return None,
        }
        if pos > end {
            return None;
        }
    }
    None
}

/// The canonical bytes of the message in `input` when it parses completely as a message and every
/// part re-encodes to the same bytes. This is the rule that decides whether a length-delimited
/// field is a nested message.
pub(super) fn canonical_message(input: &[u8], depth: usize, max_depth: usize) -> Option<Vec<u8>> {
    if depth > max_depth {
        return None;
    }
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0_usize;
    while pos < input.len() {
        let start = pos;
        let (tag, used) = read_varint(input, pos)?;
        pos = pos.checked_add(used)?;
        let number = tag >> 3;
        let wire = u8::try_from(tag & 7).ok()?;
        if !valid_field_number(number) {
            return None;
        }
        encode_varint(field_tag(number, wire), &mut out);
        match wire {
            0 => {
                let (value, used) = read_varint(input, pos)?;
                pos = pos.checked_add(used)?;
                encode_varint(value, &mut out);
            }
            1 => {
                let bytes = input.get(pos..pos.checked_add(8)?)?;
                out.extend_from_slice(bytes);
                pos = pos.checked_add(8)?;
            }
            2 => {
                let (length, used) = read_varint(input, pos)?;
                pos = pos.checked_add(used)?;
                let length = usize::try_from(length).ok()?;
                let payload = input.get(pos..pos.checked_add(length)?)?;
                pos = pos.checked_add(length)?;
                let canonical = canonical_message(payload, depth + 1, max_depth)
                    .unwrap_or_else(|| payload.to_vec());
                encode_varint(len_u64(canonical.len()), &mut out);
                out.extend_from_slice(&canonical);
            }
            3 => {
                let after = scan_group(input, pos, input.len(), number, depth + 1, max_depth)?;
                out.extend_from_slice(input.get(start..after)?);
                pos = after;
            }
            4 => return None,
            5 => {
                let bytes = input.get(pos..pos.checked_add(4)?)?;
                out.extend_from_slice(bytes);
                pos = pos.checked_add(4)?;
            }
            _ => return None,
        }
    }
    (out == input).then_some(out)
}

/// Decodes a whole document.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    let mut parser = Parser {
        input,
        doc: SaveDoc::with_node_limit(limits.max_nodes),
        max_depth: limits.max_depth,
    };
    let root = parser.message(0, input.len(), 0)?;
    parser.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((parser.doc, params))
}

/// True when a complete, non-trivial message was parsed.
pub(super) fn plausible(input: &[u8], limits: &Limits) -> bool {
    if input.len() < 2 {
        return false;
    }
    match decode(input, limits) {
        Ok((doc, _)) => matches!(
            doc.get(doc.root()).map(|node| &node.value),
            Some(Value::Array(fields)) if !fields.is_empty()
        ),
        Err(_) => false,
    }
}

struct Parser<'a> {
    input: &'a [u8],
    doc: SaveDoc,
    max_depth: usize,
}

impl Parser<'_> {
    fn take(&self, start: usize, end: usize) -> Result<&[u8]> {
        self.input
            .get(start..end)
            .ok_or_else(|| parse_error("the field is cut short", start))
    }

    fn push(
        &mut self,
        value: Value,
        hint_name: &str,
        span: Option<(usize, usize)>,
        read_only: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(hint_name));
        if let Some((start, end)) = span {
            node.span = Some(ByteSpan {
                start: len_u64(start),
                end: len_u64(end),
            });
        }
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn message(&mut self, start: usize, end: usize, depth: usize) -> Result<NodeId> {
        if depth > self.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("protobuf nesting is deeper than {} levels", self.max_depth),
            ));
        }
        let mut pos = start;
        let mut fields = Vec::new();
        while pos < end {
            let field_start = pos;
            let (tag, used) =
                read_varint(self.input, pos).ok_or_else(|| parse_error("bad tag varint", pos))?;
            pos = pos
                .checked_add(used)
                .ok_or_else(|| parse_error("the tag is too long", field_start))?;
            let number = tag >> 3;
            let wire = u8::try_from(tag & 7).unwrap_or(7);
            if !valid_field_number(number) {
                return Err(parse_error("invalid protobuf field number", field_start));
            }
            let value = match wire {
                0 => {
                    let payload_start = pos;
                    let (value, used) = read_varint(self.input, pos)
                        .ok_or_else(|| parse_error("bad varint value", pos))?;
                    pos = pos
                        .checked_add(used)
                        .ok_or_else(|| parse_error("the varint is too long", payload_start))?;
                    if pos > end {
                        return Err(parse_error(
                            "the field runs past its message",
                            payload_start,
                        ));
                    }
                    self.push(
                        Value::Int {
                            v: i128::from(value),
                            width: IntWidth::U64,
                        },
                        "varint",
                        Some((payload_start, pos)),
                        false,
                    )?
                }
                1 => {
                    let payload_start = pos;
                    let payload_end = payload_start
                        .checked_add(8)
                        .filter(|end_pos| *end_pos <= end)
                        .ok_or_else(|| parse_error("the fixed64 value is cut short", pos))?;
                    let bytes = self.take(payload_start, payload_end)?;
                    let value = u64::from_le_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| parse_error("the fixed64 value is cut short", pos))?,
                    );
                    pos = payload_end;
                    self.push(
                        Value::Int {
                            v: i128::from(value),
                            width: IntWidth::U64,
                        },
                        "fixed64",
                        Some((payload_start, payload_end)),
                        false,
                    )?
                }
                2 => {
                    let length_start = pos;
                    let (length, used) = read_varint(self.input, pos)
                        .ok_or_else(|| parse_error("bad length varint", pos))?;
                    pos = pos
                        .checked_add(used)
                        .ok_or_else(|| parse_error("the length is too long", length_start))?;
                    let length = usize::try_from(length)
                        .map_err(|_| parse_error("the length does not fit this platform", pos))?;
                    let payload_start = pos;
                    let payload_end = payload_start
                        .checked_add(length)
                        .filter(|end_pos| *end_pos <= end)
                        .ok_or_else(|| {
                            parse_error("the length-delimited field is cut short", pos)
                        })?;
                    let payload = self.take(payload_start, payload_end)?;
                    pos = payload_end;
                    let is_message =
                        canonical_message(payload, depth + 1, self.max_depth).is_some();
                    if is_message {
                        self.message(payload_start, payload_end, depth + 1)?
                    } else if let Ok(text) = std::str::from_utf8(payload) {
                        self.push(
                            Value::Str(text.to_owned()),
                            "string",
                            Some((payload_start, payload_end)),
                            false,
                        )?
                    } else {
                        self.push(
                            Value::Bytes(payload.to_vec()),
                            "bytes",
                            Some((payload_start, payload_end)),
                            true,
                        )?
                    }
                }
                3 => {
                    let after = scan_group(self.input, pos, end, number, depth + 1, self.max_depth)
                        .ok_or_else(|| parse_error("unterminated or invalid group", pos))?;
                    let payload = self.take(field_start, after)?;
                    pos = after;
                    self.push(
                        Value::Bytes(payload.to_vec()),
                        "group",
                        Some((field_start, after)),
                        true,
                    )?
                }
                4 => {
                    return Err(parse_error(
                        "an end-group tag appears outside a group",
                        field_start,
                    ));
                }
                5 => {
                    let payload_start = pos;
                    let payload_end = payload_start
                        .checked_add(4)
                        .filter(|end_pos| *end_pos <= end)
                        .ok_or_else(|| parse_error("the fixed32 value is cut short", pos))?;
                    let bytes = self.take(payload_start, payload_end)?;
                    let value = u32::from_le_bytes(
                        bytes
                            .try_into()
                            .map_err(|_| parse_error("the fixed32 value is cut short", pos))?,
                    );
                    pos = payload_end;
                    self.push(
                        Value::Int {
                            v: i128::from(value),
                            width: IntWidth::U32,
                        },
                        "fixed32",
                        Some((payload_start, payload_end)),
                        false,
                    )?
                }
                _ => {
                    return Err(parse_error(
                        format!("unknown wire type {wire}"),
                        field_start,
                    ));
                }
            };
            let number_node = self.push(
                Value::Int {
                    v: i128::from(number),
                    width: IntWidth::U32,
                },
                "number",
                None,
                true,
            )?;
            let wire_node = self.push(
                Value::Int {
                    v: i128::from(wire),
                    width: IntWidth::U8,
                },
                "wire",
                None,
                true,
            )?;
            let entries = vec![
                ("number".to_owned(), number_node),
                ("wire".to_owned(), wire_node),
                ("value".to_owned(), value),
            ];
            let mut field = Node::new(Value::Object(entries));
            field.type_hint = Some(hint("field"));
            field.flags.insert(NodeFlags::READ_ONLY);
            field.span = Some(ByteSpan {
                start: len_u64(field_start),
                end: len_u64(pos),
            });
            fields.push(self.doc.push(field)?);
        }
        let mut node = Node::new(Value::Array(fields));
        node.type_hint = Some(hint("message"));
        node.flags.insert(NodeFlags::READ_ONLY);
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
        self.doc.push(node)
    }
}
