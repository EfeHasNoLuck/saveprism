//! Writing the parts of an Adventure Creator save back into one file.

use saveprism_core::{Codec, Error, Layer, ParamValue, Params, Result, SaveDoc};

use super::AC_WRITE;
use super::read::{PartKind, data_child};
use crate::json::JsonCodec;
use crate::layers::Base64Layer;
use crate::nrbf::NrbfCodec;
use crate::xml::XmlCodec;

/// Encodes every part and joins them with the recorded separators.
pub(crate) fn encode_parts(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let Some(ParamValue::List(parts)) = params.get("parts") else {
        return Err(Error::new(
            AC_WRITE,
            "the Adventure Creator part parameters are missing",
        ));
    };
    let Some(ParamValue::List(separators)) = params.get("separators") else {
        return Err(Error::new(
            AC_WRITE,
            "the Adventure Creator separator parameters are missing",
        ));
    };
    let root = doc.root();
    let wrappers = match doc.get(root).map(|node| &node.value) {
        Some(saveprism_core::Value::Array(items)) => items.as_slice(),
        _ => {
            return Err(Error::new(
                AC_WRITE,
                "the Adventure Creator document root is not a list of parts",
            ));
        }
    };
    if wrappers.len() != parts.len() {
        return Err(Error::new(
            AC_WRITE,
            "the Adventure Creator document no longer has the same number of parts",
        ));
    }
    if separators.len().saturating_add(1) != parts.len() {
        return Err(Error::new(
            AC_WRITE,
            "the Adventure Creator separators do not match the parts",
        ));
    }

    let mut out: Vec<u8> = Vec::new();
    for (index, wrapper) in wrappers.iter().enumerate() {
        let Some(ParamValue::Table(table)) = parts.get(index) else {
            return Err(Error::new(
                AC_WRITE,
                "a part parameter entry is not a table",
            ));
        };
        let Some(ParamValue::Table(child_params)) = table.get("params") else {
            return Err(Error::new(AC_WRITE, "a part has no codec parameters"));
        };
        let hint = doc
            .get(*wrapper)
            .and_then(|node| node.type_hint.as_ref())
            .map(|hint| hint.as_str())
            .unwrap_or_default();
        let Some(kind) = super::read::kind_from_hint(hint) else {
            return Err(Error::new(AC_WRITE, "a part wrapper has no known format"));
        };
        let child = data_child(doc, *wrapper)
            .ok_or_else(|| Error::new(AC_WRITE, "a part wrapper has no data child"))?;
        let mut temp = SaveDoc::new();
        let copied = crate::text_common::copy_graph(doc, &mut temp, child, AC_WRITE)?;
        temp.set_root(copied);
        let bytes = encode_part(&temp, child_params, kind, table)?;
        out.extend_from_slice(&bytes);
        if let Some(ParamValue::Str(separator)) = separators.get(index) {
            out.extend_from_slice(separator.as_bytes());
        }
    }
    Ok(out)
}

fn encode_part(doc: &SaveDoc, params: &Params, kind: PartKind, part: &Params) -> Result<Vec<u8>> {
    match kind {
        PartKind::Json => {
            let current = crate::json::compact_text(doc, doc.root())?;
            let current = String::from_utf8(current)
                .map_err(|_| Error::new(AC_WRITE, "a JSON part is not UTF-8"))?;
            match (part.opt_str("canonical")?, part.opt_str("original")?) {
                (Some(canonical), Some(original)) if canonical == current => {
                    Ok(original.as_bytes().to_vec())
                }
                _ => JsonCodec.encode(doc, params),
            }
        }
        PartKind::Xml => XmlCodec.encode(doc, params),
        PartKind::Nrbf => {
            let Some(ParamValue::Table(base64_params)) = params.get("base64") else {
                return Err(Error::new(
                    AC_WRITE,
                    "the Base64 part parameters are missing",
                ));
            };
            let Some(ParamValue::Table(nrbf_params)) = params.get("nrbf") else {
                return Err(Error::new(
                    AC_WRITE,
                    "the BinaryFormatter part parameters are missing",
                ));
            };
            let inner = NrbfCodec.encode(doc, nrbf_params)?;
            Base64Layer.encode(&inner, base64_params)
        }
    }
}
