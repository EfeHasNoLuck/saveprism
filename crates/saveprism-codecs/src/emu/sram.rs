//! Plain `.srm` battery saves (RetroArch's in-game save RAM), see `mod.rs`.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, IntWidth, Node, NodeFlags,
    Params, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The edited SRAM changed its length.
pub const SRAM_WRITE: ErrorCode = ErrorCode::new("SP-SRAM-001");

const ORIGINAL_LEN: &str = "original_len";

/// The `format.emu-sram` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct EmulatorSramCodec;

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("EMU:{name}"))
}

impl Codec for EmulatorSramCodec {
    fn id(&self) -> &'static str {
        "format.emu-sram"
    }

    /// There is no magic; only a `.srm` name plus a plausible battery size is a weak hint.
    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".srm"));
        let plausible_size =
            (512..=8 * 1024 * 1024).contains(&input.len()) && input.len().is_multiple_of(512);
        if named && plausible_size {
            Confidence::new(0.2)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let mut doc = ctx.new_doc();
        let mut size = Node::new(Value::Int {
            v: i128::try_from(input.len()).unwrap_or(i128::MAX),
            width: IntWidth::U32,
        });
        size.type_hint = Some(hint("size"));
        // The size is the file length, shown for information; the battery size cannot change.
        size.flags.insert(NodeFlags::READ_ONLY);
        let size = doc.push(size)?;
        let mut data = Node::new(Value::Bytes(input.to_vec()));
        data.type_hint = Some(hint("sram"));
        data.span = Some(ByteSpan {
            start: 0,
            end: len_u64(input.len()),
        });
        let data = doc.push(data)?;
        let mut root = Node::new(Value::Object(vec![
            ("size".to_owned(), size),
            ("data".to_owned(), data),
        ]));
        root.type_hint = Some(hint("sram-file"));
        let root = doc.push(root)?;
        doc.set_root(root);
        let mut params = Params::new();
        params.set(ORIGINAL_LEN, i64::try_from(input.len()).unwrap_or(i64::MAX));
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let original_len = params.int(ORIGINAL_LEN)?;
        let root = doc
            .get(doc.root())
            .ok_or_else(|| Error::new(SRAM_WRITE, "the SRAM root is missing"))?;
        let entries = match &root.value {
            Value::Object(entries) => entries,
            _ => return Err(Error::new(SRAM_WRITE, "the SRAM root is not an object")),
        };
        let data = entries
            .iter()
            .find(|(key, _)| key == "data")
            .map(|(_, id)| *id)
            .ok_or_else(|| Error::new(SRAM_WRITE, "the SRAM data is missing"))?;
        let bytes = match doc.get(data).map(|node| &node.value) {
            Some(Value::Bytes(bytes)) => bytes,
            _ => return Err(Error::new(SRAM_WRITE, "the SRAM data is not a byte string")),
        };
        if i64::try_from(bytes.len()).unwrap_or(i64::MAX) != original_len {
            return Err(Error::new(
                SRAM_WRITE,
                "the SRAM data changed its length (a battery save has a fixed size)",
            ));
        }
        Ok(bytes.clone())
    }
}
