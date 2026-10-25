//! `format.yaml`: YAML 1.2 save/config files.
//!
//! Tokens come from `saphyr-parser` (MIT OR Apache-2.0), which reports the byte span of every
//! event; the tree and the exact-write spans are built here from those positions. Anchors, aliases
//! and explicit tags are read-only; the parser never constructs application objects.
//!
//! # Tree
//!
//! The root is a read-only `Array` of documents (`YAML:documents`). A mapping is a read-only
//! `Object` (`YAML:mapping`), a sequence a read-only `Array` (`YAML:sequence`); a repeated key is
//! numbered in order (`name#2`). A scalar hint is `YAML:<style>:<type>`, where the style is
//! `plain`, `single`, `double`, `literal` or `folded` and the type is `string`, `int`, `float`,
//! `bool` or `null`. Plain, single-quoted, double-quoted and literal-block scalars are editable
//! and keep their style; folded blocks, aliases (`YAML:alias`) and anchored or tagged nodes are
//! read-only.
//!
//! # Exact writing
//!
//! The original text is the codec parameter `original`. Writing renders each edited scalar in the
//! style it had (a plain string that would change type is quoted) and replaces only its token
//! span; comments, indentation, order, anchors and line endings stay untouched.

#[cfg(test)]
pub mod tests;

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, ErrorCode, ParamValue, Params, Registry, Result,
    SaveDoc, SniffCtx,
};

/// The input is not YAML.
pub const YAML_PARSE: ErrorCode = ErrorCode::new("SP-YAML-001");
/// The document cannot be written back.
pub const YAML_WRITE: ErrorCode = ErrorCode::new("SP-YAML-002");

pub(crate) const PREFIX: &str = "YAML:";

/// Registers `format.yaml`.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(YamlCodec));
}

/// The `format.yaml` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct YamlCodec;

impl Codec for YamlCodec {
    fn id(&self) -> &'static str {
        "format.yaml"
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
        if name.as_deref().is_some_and(|name| name.ends_with(".csv")) {
            return Confidence::NONE;
        }
        let named = name
            .as_deref()
            .is_some_and(|name| name.ends_with(".yaml") || name.ends_with(".yml"));
        let structured = start.starts_with("---")
            || body.lines().any(|line| {
                let line = line.trim_start();
                let mapping = line.find(':').is_some_and(|colon| {
                    colon + 1 >= line.len() || line.as_bytes().get(colon + 1) != Some(&b'/')
                }) && !line.starts_with('#');
                mapping || line.starts_with("- ") || line == "-"
            });
        if !structured && !named {
            return Confidence::NONE;
        }
        if !read::well_formed(body) {
            return Confidence::NONE;
        }
        if named {
            Confidence::new(0.85)
        } else if start.starts_with("---") {
            Confidence::new(0.72)
        } else {
            Confidence::new(0.55)
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let decoded = crate::text_encoding::decode_sniff(input)
            .ok_or_else(|| Error::new(YAML_PARSE, "YAML text encoding could not be detected"))?;
        let text = decoded.text.as_str();
        let doc = read::parse(text, 0, ctx)?;
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("text_encoding", decoded.encoding.name());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(YAML_WRITE, "the original YAML text is missing"));
        };
        let decoded = crate::text_common::decode_original(original, YAML_WRITE)?;
        let mut patches = write::encode(doc, decoded.text.as_bytes())?;
        crate::text_common::apply_encoded_patches(original, &decoded, YAML_WRITE, &mut patches)
    }
}

pub mod read;
pub mod write;
