//! `format.raw`: a read-only leaf for unknown binary data inside a container.
//!
//! Containers such as PS1/PS2 memory cards and SharkPort snapshots often hold a game's own
//! binary save, which no codec understands yet. The container layer still has to hand those
//! bytes to the pipeline, and the pipeline needs a final codec. This codec accepts only inputs
//! that arrived through a layer (`SniffCtx::depth > 0`) with a very low confidence, so it never
//! claims a file by itself and never outranks a real codec. The document is a read-only summary;
//! the original bytes are kept and written back unchanged.
//!
//! Errors: `SP-RAW-001` when encoding without the saved source bytes.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, Params, Registry,
    Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The codec was asked to encode without the original bytes.
pub const RAW_SOURCE: ErrorCode = ErrorCode::new("SP-RAW-001");

/// Parameter: the original bytes of the unknown payload.
pub const SOURCE_PARAM: &str = "source";

/// Registers the read-only raw leaf.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(RawCodec));
}

/// The read-only unknown-binary leaf.
#[derive(Clone, Copy, Debug, Default)]
pub struct RawCodec;

impl Codec for RawCodec {
    fn id(&self) -> &'static str {
        "format.raw"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if ctx.depth == 0 || input.len() < 512 {
            Confidence::NONE
        } else {
            Confidence::new(0.05)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let mut doc = ctx.new_doc();
        let mut leaf = Node::new(Value::Str(format!(
            "unknown binary data, {} bytes",
            input.len()
        )));
        leaf.type_hint = Some(TypeHint::new("RAW:binary"));
        leaf.flags.insert(NodeFlags::READ_ONLY);
        let child = doc.push(leaf)?;
        let mut root = Node::new(Value::Object(vec![("binary".to_string(), child)]));
        root.type_hint = Some(TypeHint::new("RAW:payload"));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        let mut params = Params::new();
        params.set(SOURCE_PARAM, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, _doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        params
            .bytes(SOURCE_PARAM)
            .map(|bytes| bytes.to_vec())
            .map_err(|_| {
                Error::new(
                    RAW_SOURCE,
                    "the original bytes of the unknown payload are missing",
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::NodeFlags;

    use super::*;

    #[test]
    fn only_inside_a_layer() {
        assert_eq!(
            RawCodec.sniff(b"anything", &SniffCtx::default()),
            Confidence::NONE
        );
        let inside = SniffCtx {
            file_name: None,
            depth: 1,
        };
        assert!(RawCodec.sniff(&[0_u8; 512], &inside) > Confidence::NONE);
        assert_eq!(RawCodec.sniff(b"short", &inside), Confidence::NONE);
    }

    #[test]
    fn the_leaf_is_read_only_and_writes_the_same_bytes() {
        let data = b"\x00\x01unknown\xff";
        let limits = saveprism_core::Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        let decoded = RawCodec.decode(data, &mut ctx).unwrap();
        let root = decoded.doc.get(decoded.doc.root()).unwrap();
        assert!(root.flags.contains(NodeFlags::READ_ONLY));
        assert_eq!(
            RawCodec.encode(&decoded.doc, &decoded.params).unwrap(),
            data
        );
        let missing = Params::new();
        assert_eq!(
            RawCodec.encode(&decoded.doc, &missing).unwrap_err().code(),
            RAW_SOURCE
        );
    }
}
