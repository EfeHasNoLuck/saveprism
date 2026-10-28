//! `format.scs-sii`: SCS Software unit files (Euro Truck Simulator 2, American Truck Simulator:
//! `game.sii`, `info.sii`, `profile.sii`, and mod `.sii` files).
//!
//! The plain-text form is written from the public SCS Modding Wiki "Units" page
//! (`https://modding.scssoft.com/wiki/Documentation/Engine/Units`, checked 2026-10-26) and the
//! engine "Formats" page (`.sii` is "Text and Binary"). The encrypted form and the `g_save_format`
//! hint come from the same wiki's configuration-variable page
//! (`.../Configuration_variables/g_save_format`): 2 forces text saves, 3 writes both. The binary
//! form's magic and version are the two public facts documented by the MIT `Trucky/sii-decrypt-ts`
//! reader (`DOCS.md`: `SIIB`, versions 1 to 3); only that structure is used here, and the file is
//! kept read-only because the clean-room rules forbid reading the GPL implementations that
//! describe the rest.
//!
//! # Text tree
//!
//! | SII text | Node | Hint | Editable |
//! |---|---|---|---|
//! | file | `Object` of units by name | `SCS-SII:file` | no |
//! | unit | `Object` with `class` and `attributes` | `SCS-SII:unit` | no |
//! | attribute | `Object` with `name` and `value` | `SCS-SII:attribute` | its value |
//! | `true` / `false` | `Bool` | `SCS-SII:bool` | yes |
//! | integer | `Int` | `SCS-SII:int` | yes |
//! | float or `&3f800000` | `Float` | `SCS-SII:float` | yes |
//! | `"string"` | `Str` | `SCS-SII:string` | yes |
//! | token / pointer | `Str` | `SCS-SII:token` | yes |
//! | `nil` / `null` | `Null` | `SCS-SII:nil` | no |
//! | `(x, y, z)` / placement | `Str` | `SCS-SII:vector` | no |
//!
//! Comments (`#`, `//`, `/* ... */`), spacing, blank lines and attribute order are kept only as
//! bytes of the original text. `@include` lines are ignored (the file is not followed) and stay
//! untouched.
//!
//! # Exact writing
//!
//! The original text is kept as the parameter `original`. Every editable value records the byte
//! span of its token; writing replaces only tokens an edit changed. An unchanged file therefore
//! gives its original bytes.
//!
//! # Binary and encrypted forms
//!
//! `BSII`/`SIIB` binary saves open as a read-only tree (version and the whole payload). `ScsC` /
//! `SrcC` saves are encrypted: they open read-only with a `hint` node that tells the player to set
//! `g_save_format` to 2 (or 3) in the game, load and save once, and open the resulting plain text.
//! No key is built into SavePrism (CONTRIBUTING.md, "Clean room"); the game itself writes the plain text.

mod read;
mod write;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, Node, NodeFlags, Params, Registry,
    Result, SaveDoc, SniffCtx, TypeHint, Value,
};

/// The text is not UTF-8, uses a token this parser does not read, or ends too early.
pub const SCS_SII_PARSE: ErrorCode = ErrorCode::new("SP-SCS-001");
/// A document cannot be written as SII (a value is not representable, a span is missing).
pub const SCS_SII_WRITE: ErrorCode = ErrorCode::new("SP-SCS-002");

/// Prefix of every hint this codec writes.
pub const HINT_PREFIX: &str = "SCS-SII:";
/// Parameter holding the original text or bytes.
pub(crate) const ORIGINAL: &str = "original";

const BINARY_MAGIC_A: &[u8; 4] = b"BSII";
const BINARY_MAGIC_B: &[u8; 4] = b"SIIB";
const ENCRYPTED_MAGIC_A: &[u8; 4] = b"ScsC";
const ENCRYPTED_MAGIC_B: &[u8; 4] = b"SrcC";

/// Registers the text, binary and encrypted SII codecs.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(ScsSiiCodec));
    reg.add_codec(Arc::new(ScsSiiBinaryCodec));
    reg.add_codec(Arc::new(ScsSiiEncryptedCodec));
}

/// The `format.scs-sii` text codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScsSiiCodec;

impl Codec for ScsSiiCodec {
    fn id(&self) -> &'static str {
        "format.scs-sii"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        if !read::plausible(input) {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".sii"));
        if named {
            Confidence::new(0.97)
        } else {
            Confidence::new(0.9)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let (doc, params) = read::decode(input, ctx.limits())?;
        ctx.charge_nodes(len_u64(doc.len().saturating_sub(1)))?;
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        write::encode(doc, params)
    }
}

/// The `format.scs-sii-binary` read-only codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScsSiiBinaryCodec;

impl Codec for ScsSiiBinaryCodec {
    fn id(&self) -> &'static str {
        "format.scs-sii-binary"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(BINARY_MAGIC_A) || input.starts_with(BINARY_MAGIC_B) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        if input.len() < 8 {
            return Err(Error::new(
                SCS_SII_PARSE,
                "the binary SII header is cut short",
            ));
        }
        let version = u32::from_le_bytes(
            input
                .get(4..8)
                .ok_or_else(|| Error::new(SCS_SII_PARSE, "the binary SII header is cut short"))?
                .try_into()
                .map_err(|_| Error::new(SCS_SII_PARSE, "the binary SII header is cut short"))?,
        );
        let mut doc = ctx.new_doc();
        let version_node = push_read_only(
            &mut doc,
            Value::Int {
                v: i128::from(version),
                width: saveprism_core::IntWidth::U32,
            },
            "SCS-SIIB:version",
        )?;
        let payload_node =
            push_read_only(&mut doc, Value::Bytes(input.to_vec()), "SCS-SIIB:payload")?;
        let mut root = Node::new(Value::Object(vec![
            ("version".to_owned(), version_node),
            ("payload".to_owned(), payload_node),
        ]));
        root.type_hint = Some(TypeHint::new("SCS-SIIB:file"));
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
            .map_err(|_| Error::new(SCS_SII_WRITE, "the original binary SII bytes are missing"))
    }
}

/// The `format.scs-sii-encrypted` read-only codec: opens the file with an explanation instead of
/// guessing a key.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScsSiiEncryptedCodec;

const ENCRYPTED_HINT: &str = "This SII save is encrypted. In the game's config.cfg set g_save_format to 2 (force text) or 3 (text plus binary), load the profile and save once; the plain game.sii can then be opened here. SavePrism does not contain the game's key.";

impl Codec for ScsSiiEncryptedCodec {
    fn id(&self) -> &'static str {
        "format.scs-sii-encrypted"
    }

    fn sniff(&self, input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
        if input.starts_with(ENCRYPTED_MAGIC_A) || input.starts_with(ENCRYPTED_MAGIC_B) {
            Confidence::new(0.95)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let magic = input
            .get(..4)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        let mut doc = ctx.new_doc();
        let magic_node = push_read_only(&mut doc, Value::Str(magic), "SCS-SII:encrypted-format")?;
        let status_node = push_read_only(
            &mut doc,
            Value::Str("encrypted (read-only)".to_owned()),
            "SCS-SII:encrypted-status",
        )?;
        let hint_node = push_read_only(
            &mut doc,
            Value::Str(ENCRYPTED_HINT.to_owned()),
            "SCS-SII:encrypted-hint",
        )?;
        let payload_node = push_read_only(
            &mut doc,
            Value::Bytes(input.to_vec()),
            "SCS-SII:encrypted-payload",
        )?;
        let mut root = Node::new(Value::Object(vec![
            ("format".to_owned(), magic_node),
            ("status".to_owned(), status_node),
            ("hint".to_owned(), hint_node),
            ("payload".to_owned(), payload_node),
        ]));
        root.type_hint = Some(TypeHint::new("SCS-SII:encrypted"));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        let mut params = Params::new();
        params.set(ORIGINAL, input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, _doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        params.bytes(ORIGINAL).map(<[u8]>::to_vec).map_err(|_| {
            Error::new(
                SCS_SII_WRITE,
                "the original encrypted SII bytes are missing",
            )
        })
    }
}

fn push_read_only(
    doc: &mut SaveDoc,
    value: Value,
    type_hint: &str,
) -> Result<saveprism_core::NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(type_hint));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}
