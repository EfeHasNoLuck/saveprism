//! RetroArch save states (`.state`, `.state1`, ...), see `mod.rs`.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, IntWidth, Node, NodeFlags,
    NodeId, Params, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// A `RASTATE` file is cut short or a block length points outside the file.
pub const RA_PARSE: ErrorCode = ErrorCode::new("SP-RASTATE-001");

const MAGIC: &[u8; 7] = b"RASTATE";
const RZIP: &[u8; 5] = b"#RZIP";
const ORIGINAL: &str = "original";
/// Payloads are padded to a multiple of this many bytes in a `RASTATE` file.
const ALIGN: usize = 8;

/// The `format.retroarch-state` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct RetroArchStateCodec;

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(RA_PARSE, message).with_offset(len_u64(at))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("RA:{name}"))
}

fn push(
    doc: &mut SaveDoc,
    value: Value,
    kind: &str,
    span: Option<(usize, usize)>,
    read_only: bool,
) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(hint(kind));
    if let Some((start, end)) = span {
        node.span = Some(ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
    }
    if read_only {
        node.flags.insert(NodeFlags::READ_ONLY);
    }
    doc.push(node)
}

fn compressed(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<Decoded> {
    let mut doc = ctx.new_doc();
    let status = push(
        &mut doc,
        Value::Str("compressed RZIP state (read-only)".to_owned()),
        "status",
        None,
        true,
    )?;
    let hint_node = push(
        &mut doc,
        Value::Str(
            "RetroArch wrote this state in its RZIP container. Turn off Settings > Saving > Save State File Compression, save the state again, or load it in RetroArch and save an uncompressed copy.".to_owned(),
        ),
        "hint",
        None,
        true,
    )?;
    let payload = push(
        &mut doc,
        Value::Bytes(input.to_vec()),
        "payload",
        None,
        true,
    )?;
    let mut root = Node::new(Value::Object(vec![
        ("status".to_owned(), status),
        ("hint".to_owned(), hint_node),
        ("payload".to_owned(), payload),
    ]));
    root.type_hint = Some(hint("compressed"));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root)?;
    doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok(Decoded { doc, params })
}

impl Codec for RetroArchStateCodec {
    fn id(&self) -> &'static str {
        "format.retroarch-state"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(MAGIC) {
            Confidence::new(0.95)
        } else if input.starts_with(RZIP) {
            Confidence::new(0.5)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if input.starts_with(RZIP) {
            return compressed(input, ctx);
        }
        if input.len() < 8 || !input.starts_with(MAGIC) {
            return Err(parse_error("missing the RASTATE header", 0));
        }
        let version = *input
            .get(7)
            .ok_or_else(|| parse_error("the RASTATE version byte is missing", 7))?;
        if version != 1 {
            return Err(parse_error(
                format!("unsupported RASTATE version {version}"),
                7,
            ));
        }
        let mut doc = ctx.new_doc();
        let version_node = push(
            &mut doc,
            Value::Int {
                v: i128::from(version),
                width: IntWidth::U8,
            },
            "version",
            None,
            true,
        )?;
        let mut entries = Vec::new();
        let mut pos = 8_usize;
        while pos < input.len() {
            let header_end = pos
                .checked_add(8)
                .filter(|end| *end <= input.len())
                .ok_or_else(|| parse_error("a state block header is cut short", pos))?;
            let header = input
                .get(pos..header_end)
                .ok_or_else(|| parse_error("a state block header is cut short", pos))?;
            let tag = std::str::from_utf8(header.get(..4).unwrap_or_default())
                .map_err(|_| parse_error("a state block tag is not ASCII", pos))?
                .to_owned();
            let length = u32::from_le_bytes(
                header
                    .get(4..8)
                    .ok_or_else(|| parse_error("a state block length is cut short", pos))?
                    .try_into()
                    .map_err(|_| parse_error("a state block length is cut short", pos))?,
            );
            let length = usize::try_from(length)
                .map_err(|_| parse_error("a state block length is too large", pos))?;
            let data_start = header_end;
            let data_end = data_start
                .checked_add(length)
                .filter(|end| *end <= input.len())
                .ok_or_else(|| parse_error("a state block payload is cut short", data_start))?;
            let padded = length
                .checked_add((ALIGN - length % ALIGN) % ALIGN)
                .ok_or_else(|| parse_error("a state block length overflows", pos))?;
            let next = header_end
                .checked_add(padded)
                .filter(|end| *end <= input.len())
                .ok_or_else(|| parse_error("a state block padding is cut short", data_start))?;
            let tag_node = push(&mut doc, Value::Str(tag.clone()), "block-tag", None, true)?;
            let size_node = push(
                &mut doc,
                Value::Int {
                    v: i128::try_from(length).unwrap_or(i128::MAX),
                    width: IntWidth::U32,
                },
                "block-size",
                None,
                true,
            )?;
            let data_node = push(
                &mut doc,
                Value::Bytes(input.get(data_start..data_end).unwrap_or_default().to_vec()),
                "block-data",
                Some((data_start, data_end)),
                true,
            )?;
            let mut entry = Node::new(Value::Object(vec![
                ("tag".to_owned(), tag_node),
                ("size".to_owned(), size_node),
                ("data".to_owned(), data_node),
            ]));
            entry.type_hint = Some(hint("block"));
            entry.flags.insert(NodeFlags::READ_ONLY);
            entries.push(doc.push(entry)?);
            pos = next;
        }
        let blocks = push(&mut doc, Value::Array(entries), "blocks", None, true)?;
        let mut root = Node::new(Value::Object(vec![
            ("version".to_owned(), version_node),
            ("blocks".to_owned(), blocks),
        ]));
        root.type_hint = Some(hint("state"));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, _doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        params
            .bytes(ORIGINAL)
            .map(<[u8]>::to_vec)
            .map_err(|_| Error::new(RA_PARSE, "the original RetroArch state bytes are missing"))
    }
}
