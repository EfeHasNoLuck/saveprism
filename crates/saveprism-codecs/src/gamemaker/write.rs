//! Canonical writer for a GameMaker `ds_map` tree (uppercase hexadecimal text).

use saveprism_core::{Error, Result, SaveDoc, Value};

use super::{GM_WRITE, MAGIC};

/// Renders the whole document as the hexadecimal `ds_map_write` text.
pub(crate) fn render(doc: &SaveDoc) -> Result<Vec<u8>> {
    let root = doc
        .get(doc.root())
        .ok_or_else(|| Error::new(GM_WRITE, "the GameMaker ds_map root is missing"))?;
    let Value::Map(entries) = &root.value else {
        return Err(Error::new(
            GM_WRITE,
            "the GameMaker ds_map root is not a map",
        ));
    };
    let count = u32::try_from(entries.len())
        .map_err(|_| Error::new(GM_WRITE, "the GameMaker ds_map has too many entries"))?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    for (key, value) in entries {
        render_object(doc, *key, true, &mut bytes)?;
        render_object(doc, *value, false, &mut bytes)?;
    }
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        text.push_str(&format!("{byte:02X}"));
    }
    Ok(text.into_bytes())
}

fn hint(node: &saveprism_core::Node) -> &str {
    node.type_hint
        .as_ref()
        .map_or("", saveprism_core::TypeHint::as_str)
}

fn render_object(
    doc: &SaveDoc,
    id: saveprism_core::NodeId,
    key: bool,
    out: &mut Vec<u8>,
) -> Result<()> {
    let node = doc
        .get(id)
        .ok_or_else(|| Error::new(GM_WRITE, "a GameMaker ds_map entry is missing"))?;
    let kind = hint(node);
    match (key, kind, &node.value) {
        (true, "GM:key:real", Value::Float { v, .. }) => {
            render_real(*v, out);
            Ok(())
        }
        (true, "GM:key:string", Value::Str(text)) => {
            render_string(text.as_bytes(), out)?;
            Ok(())
        }
        (false, "GM:real", Value::Float { v, .. }) => {
            render_real(*v, out);
            Ok(())
        }
        (false, "GM:real", Value::Int { v, .. }) => {
            render_real(*v as f64, out);
            Ok(())
        }
        (false, "GM:string", Value::Str(text)) => {
            render_string(text.as_bytes(), out)?;
            Ok(())
        }
        (false, "GM:string", Value::Bytes(bytes)) => {
            render_string(bytes, out)?;
            Ok(())
        }
        _ => Err(Error::new(
            GM_WRITE,
            format!(
                "a GameMaker ds_map {} has an unsupported type (hint {kind})",
                if key { "key" } else { "value" }
            ),
        )),
    }
}

fn render_real(value: f64, out: &mut Vec<u8>) {
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn render_string(bytes: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| Error::new(GM_WRITE, "a GameMaker ds_map string is too long"))?;
    out.extend_from_slice(&1_u32.to_le_bytes());
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}
