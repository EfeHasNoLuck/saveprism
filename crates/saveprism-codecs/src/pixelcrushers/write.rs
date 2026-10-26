//! Writing the nested records back into the outer JSON.

use saveprism_core::{Codec, Error, NodeId, ParamValue, Params, Result, SaveDoc, Value};

use super::read::{NestedKind, record_data_nodes};
use super::{PC_WRITE, lua};
use crate::json::{JsonCodec, compact_text};

pub(crate) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let nested = match params.get("nested") {
        Some(ParamValue::List(items)) => items.as_slice(),
        _ => &[],
    };
    let nodes = record_data_nodes(doc)?;
    if nodes.len() != nested.len() {
        return Err(Error::new(
            PC_WRITE,
            "the nested record list no longer matches the save records",
        ));
    }
    let mut plain = doc.clone();
    for (node, info) in nodes.iter().zip(nested) {
        let ParamValue::Table(table) = info else {
            return Err(Error::new(
                PC_WRITE,
                "a nested record parameter is not a table",
            ));
        };
        let kind = table
            .opt_str("kind")?
            .and_then(NestedKind::from_name)
            .ok_or_else(|| Error::new(PC_WRITE, "a nested record has no known kind"))?;
        let text = match kind {
            NestedKind::Raw => continue,
            NestedKind::Json => {
                let current = compact_text(&plain, *node)?;
                let current = String::from_utf8(current)
                    .map_err(|_| Error::new(PC_WRITE, "a nested JSON record is not UTF-8"))?;
                let canonical = table.opt_str("canonical")?.unwrap_or_default();
                if current == canonical {
                    table.str("original")?.to_owned()
                } else {
                    current
                }
            }
            NestedKind::Lua => {
                let original = table.str("original")?;
                let bytes = lua::write_nested(original, &plain, *node)?;
                String::from_utf8(bytes)
                    .map_err(|_| Error::new(PC_WRITE, "a written Lua record is not UTF-8"))?
            }
        };
        replace_with_string(&mut plain, *node, text)?;
    }
    JsonCodec.encode(&plain, params)
}

fn replace_with_string(doc: &mut SaveDoc, id: NodeId, text: String) -> Result<()> {
    doc.set_value(id, Value::Str(text))
}
