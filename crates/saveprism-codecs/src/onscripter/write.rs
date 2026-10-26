//! Saves ONScripter variables and descriptions by splicing changed scalars into the original bytes.

use saveprism_core::{ByteSpan, Error, ParamValue, Params, Result, SaveDoc, Value};

use crate::text_encoding::TextEncoding;

use super::{ONS_WRITE, P_ORIGINAL, parse_error};

pub(crate) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let original = match params.get(P_ORIGINAL) {
        Some(ParamValue::Bytes(bytes)) => bytes,
        _ => {
            return Err(Error::new(
                ONS_WRITE,
                "the recipe has no original ONScripter bytes to patch",
            ));
        }
    };
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get(id) else {
            continue;
        };
        if let Some(span) = node.span
            && let Some(hint) = node.type_hint.as_ref().map(|hint| hint.as_str())
        {
            let replacement = match (hint, &node.value) {
                ("ONS:number", Value::Int { v, .. }) => {
                    let value = i32::try_from(*v).map_err(|_| {
                        Error::new(ONS_WRITE, "an ONScripter number does not fit int32")
                    })?;
                    Some(value.to_le_bytes().to_vec())
                }
                ("ONS:global-string" | "ONS:save-description", Value::Str(text)) => {
                    Some(TextEncoding::ShiftJis.encode(text)?)
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                patches.push(Patch { span, replacement });
            }
        }
        stack.extend(doc.children(id));
    }
    patches.sort_by_key(|patch| patch.span.start);

    let mut out = original.clone();
    for patch in patches.iter().rev() {
        let start = usize::try_from(patch.span.start)
            .map_err(|_| parse_error("an ONScripter span does not fit this platform"))?;
        let end = usize::try_from(patch.span.end)
            .map_err(|_| parse_error("an ONScripter span does not fit this platform"))?;
        if start > end || end > out.len() {
            return Err(Error::new(
                ONS_WRITE,
                "an ONScripter value span is outside the original bytes",
            ));
        }
        out.splice(start..end, patch.replacement.iter().copied());
    }
    Ok(out)
}

struct Patch {
    span: ByteSpan,
    replacement: Vec<u8>,
}
