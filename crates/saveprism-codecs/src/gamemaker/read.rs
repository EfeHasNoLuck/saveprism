//! Parser for a GameMaker `ds_map_write` hexadecimal dump.

use saveprism_core::{
    Error, FloatWidth, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint, Value, codes,
};

use super::{GM_PARSE, MAGIC, PREFIX};

/// Converts the hexadecimal text into bytes.
pub(crate) fn hex_bytes(text: &str) -> Result<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(Error::new(
            GM_PARSE,
            "the GameMaker ds_map dump has an odd number of hex digits",
        ));
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let digits = std::str::from_utf8(pair)
            .map_err(|_| Error::new(GM_PARSE, "the GameMaker ds_map dump is not ASCII"))?;
        let byte = u8::from_str_radix(digits, 16)
            .map_err(|_| Error::new(GM_PARSE, "the GameMaker ds_map dump has a bad hex digit"))?;
        out.push(byte);
    }
    Ok(out)
}

/// Parses the bytes of a serialized map into a read-only `Map` root.
pub(crate) fn parse(bytes: &[u8], doc: &mut SaveDoc, max_nodes: u64) -> Result<NodeId> {
    let mut cursor = Cursor {
        data: bytes,
        pos: 0,
    };
    let magic = cursor.u32()?;
    if magic != MAGIC {
        return Err(Error::new(
            GM_PARSE,
            format!("the GameMaker ds_map magic is {magic}, not {MAGIC}"),
        ));
    }
    let count = u64::from(cursor.u32()?);
    if count.saturating_mul(2) > max_nodes {
        return Err(Error::new(
            codes::CORE_LIMIT_NODES,
            format!("the GameMaker ds_map has {count} entries (node limit)"),
        ));
    }
    let Ok(count) = usize::try_from(count) else {
        return Err(Error::new(
            GM_PARSE,
            "the GameMaker ds_map count is too large",
        ));
    };
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let key = cursor.object(doc, true)?;
        let value = cursor.object(doc, false)?;
        entries.push((key, value));
    }
    if cursor.pos != bytes.len() {
        return Err(Error::new(
            GM_PARSE,
            format!(
                "{} bytes follow the last GameMaker ds_map entry",
                bytes.len().saturating_sub(cursor.pos)
            ),
        ));
    }
    let mut root = Node::new(Value::Map(entries));
    root.type_hint = Some(TypeHint::new(format!("{PREFIX}map")));
    root.flags.insert(NodeFlags::READ_ONLY);
    doc.push(root)
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        let end = self
            .pos
            .checked_add(count)
            .ok_or_else(|| Error::new(GM_PARSE, "GameMaker ds_map offset overflow"))?;
        let slice = self.data.get(self.pos..end).ok_or_else(|| {
            Error::new(
                GM_PARSE,
                format!("the GameMaker ds_map is cut short at byte {}", self.pos),
            )
        })?;
        self.pos = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| Error::new(GM_PARSE, "the GameMaker ds_map is cut short"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| Error::new(GM_PARSE, "the GameMaker ds_map is cut short"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn object(&mut self, doc: &mut SaveDoc, key: bool) -> Result<NodeId> {
        let kind = self.u32()?;
        match kind {
            0 => {
                let value = f64::from_bits(self.u64()?);
                let mut node = Node::new(Value::Float {
                    v: value,
                    width: FloatWidth::F64,
                });
                node.type_hint = Some(TypeHint::new(if key {
                    format!("{PREFIX}key:real")
                } else {
                    format!("{PREFIX}real")
                }));
                if key {
                    node.flags.insert(NodeFlags::READ_ONLY);
                }
                doc.push(node)
            }
            1 => {
                let length = usize::try_from(self.u32()?)
                    .map_err(|_| Error::new(GM_PARSE, "GameMaker ds_map string is too long"))?;
                let data = self.take(length)?;
                if key {
                    let text = std::str::from_utf8(data)
                        .map_err(|_| Error::new(GM_PARSE, "a GameMaker ds_map key is not UTF-8"))?;
                    let mut node = Node::new(Value::Str(text.to_owned()));
                    node.type_hint = Some(TypeHint::new(format!("{PREFIX}key:string")));
                    node.flags.insert(NodeFlags::READ_ONLY);
                    doc.push(node)
                } else {
                    match std::str::from_utf8(data) {
                        Ok(text) => {
                            let mut node = Node::new(Value::Str(text.to_owned()));
                            node.type_hint = Some(TypeHint::new(format!("{PREFIX}string")));
                            doc.push(node)
                        }
                        Err(_) => {
                            let mut node = Node::new(Value::Bytes(data.to_vec()));
                            node.type_hint = Some(TypeHint::new(format!("{PREFIX}string")));
                            node.flags.insert(NodeFlags::READ_ONLY);
                            doc.push(node)
                        }
                    }
                }
            }
            other => Err(Error::new(
                GM_PARSE,
                format!("unsupported GameMaker ds_map value type {other}"),
            )),
        }
    }
}
