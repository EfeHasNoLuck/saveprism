//! Splitting and decoding the parts of an Adventure Creator save.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use saveprism_core::{Codec, DecodeCtx, Error, Layer, ParamValue, Params, Result, SaveDoc, Value};

use super::{AC_PARSE, AC_PART, MAIN_DIVIDER, ROOM_DIVIDER, contains, trim_start_index};
use crate::json::JsonCodec;
use crate::layers::Base64Layer;
use crate::nrbf::NrbfCodec;
use crate::xml::XmlCodec;

/// The three public AC file format handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartKind {
    /// `FileFormatHandler_Json`
    Json,
    /// `FileFormatHandler_Xml`
    Xml,
    /// `FileFormatHandler_Binary` (Base64 text around `BinaryFormatter` data)
    Nrbf,
}

impl PartKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            PartKind::Json => "json",
            PartKind::Xml => "xml",
            PartKind::Nrbf => "nrbf",
        }
    }
}

/// One decoded part and the parameters its codec needs to write it again.
pub(crate) struct PartDecoded {
    pub(crate) doc: SaveDoc,
    pub(crate) params: Params,
    pub(crate) text: String,
    /// Compact JSON of the part at decode time, so an unchanged part can be written back as its
    /// original text whatever its spacing.
    pub(crate) canonical: Option<String>,
}

/// Part bytes, the separators between them, and the part format.
pub(crate) type SplitSave<'a> = (Vec<&'a [u8]>, Vec<String>, PartKind);

/// Splits the save into part bytes and the exact separators between them.
pub(crate) fn split_parts(input: &[u8]) -> Result<SplitSave<'_>> {
    let divider = find(input, MAIN_DIVIDER).ok_or_else(|| {
        Error::new(
            AC_PARSE,
            "an Adventure Creator save joins its main and scene data with \"||\"",
        )
    })?;
    if divider == 0 {
        return Err(Error::new(AC_PARSE, "the main data before \"||\" is empty"));
    }
    let main = input.get(..divider).unwrap_or_default();
    let rest = input
        .get(divider.saturating_add(MAIN_DIVIDER.len())..)
        .unwrap_or_default();
    let kind = kind_of(main)?;

    let mut parts = vec![main];
    let mut separators = vec![String::from_utf8_lossy(MAIN_DIVIDER).into_owned()];
    if !rest.is_empty() {
        let room_split = match kind {
            PartKind::Json => true,
            PartKind::Xml | PartKind::Nrbf => find(rest, ROOM_DIVIDER).is_some(),
        };
        if room_split {
            let mut start = 0usize;
            while let Some(offset) = find(rest.get(start..).unwrap_or_default(), ROOM_DIVIDER) {
                let end = start.saturating_add(offset);
                let piece = rest.get(start..end).unwrap_or_default();
                if piece.is_empty() {
                    return Err(Error::new(AC_PARSE, "a scene record is empty"));
                }
                parts.push(piece);
                separators.push(String::from_utf8_lossy(ROOM_DIVIDER).into_owned());
                start = end.saturating_add(ROOM_DIVIDER.len());
            }
            let tail = rest.get(start..).unwrap_or_default();
            if tail.is_empty() {
                return Err(Error::new(AC_PARSE, "the last scene record is empty"));
            }
            parts.push(tail);
        } else {
            parts.push(rest);
        }
    }
    Ok((parts, separators, kind))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Infers the part format from the main data bytes.
pub(crate) fn kind_of(main: &[u8]) -> Result<PartKind> {
    let start = trim_start_index(main);
    let body = main.get(start..).unwrap_or_default();
    match body.first() {
        Some(b'{') if contains(body, b"\"mainData\"") => Ok(PartKind::Json),
        Some(b'<') if contains(body, b"<SaveData") => Ok(PartKind::Xml),
        Some(_) if looks_like_nrbf_base64(body) => Ok(PartKind::Nrbf),
        _ => Err(Error::new(
            AC_PARSE,
            "the main data is neither AC JSON (with \"mainData\"), AC XML (with <SaveData>) nor \
             Base64 BinaryFormatter data",
        )),
    }
}

pub(crate) fn looks_like_nrbf_base64(body: &[u8]) -> bool {
    let text = String::from_utf8_lossy(body);
    let cleaned: String = text
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect();
    let Ok(bytes) = STANDARD.decode(cleaned.as_bytes()) else {
        return false;
    };
    bytes.first() == Some(&0)
        && bytes.get(9..13).is_some_and(|major| major == [1, 0, 0, 0])
        && bytes.get(13..17).is_some_and(|minor| minor == [0, 0, 0, 0])
}

/// Decodes every part with the codec its format calls for.
pub(crate) fn decode_parts(
    parts: &[&[u8]],
    kind: PartKind,
    ctx: &mut DecodeCtx<'_>,
) -> Result<Vec<PartDecoded>> {
    let mut decoded = Vec::with_capacity(parts.len());
    for part in parts {
        let text = std::str::from_utf8(part)
            .map_err(|_| Error::new(AC_PART, "a save part is not UTF-8 text"))?
            .to_owned();
        let item = match kind {
            PartKind::Json => {
                let result = JsonCodec
                    .decode(part, ctx)
                    .map_err(|error| error.context("reading an Adventure Creator JSON part"))?;
                let canonical = crate::json::compact_text(&result.doc, result.doc.root())
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok());
                PartDecoded {
                    doc: result.doc,
                    params: result.params,
                    text,
                    canonical,
                }
            }
            PartKind::Xml => {
                let result = XmlCodec
                    .decode(part, ctx)
                    .map_err(|error| error.context("reading an Adventure Creator XML part"))?;
                PartDecoded {
                    doc: result.doc,
                    params: result.params,
                    text,
                    canonical: None,
                }
            }
            PartKind::Nrbf => {
                let (inner, base64_params) = Base64Layer
                    .decode(part, ctx)
                    .map_err(|error| error.context("reading an Adventure Creator Base64 part"))?;
                let result = NrbfCodec.decode(&inner, ctx).map_err(|error| {
                    error.context("reading an Adventure Creator BinaryFormatter part")
                })?;
                let mut params = Params::new();
                params.set("base64", base64_params);
                params.set("nrbf", result.params);
                PartDecoded {
                    doc: result.doc,
                    params,
                    text,
                    canonical: None,
                }
            }
        };
        decoded.push(item);
    }
    Ok(decoded)
}

/// Builds the `parts` codec parameter from the decoded parts.
pub(crate) fn parts_param(parts: &[PartDecoded]) -> ParamValue {
    let mut list = Vec::with_capacity(parts.len());
    for part in parts {
        let mut table = Params::new();
        table.set("params", part.params.clone());
        table.set("original", part.text.clone());
        if let Some(canonical) = &part.canonical {
            table.set("canonical", canonical.clone());
        }
        list.push(ParamValue::Table(table));
    }
    ParamValue::List(list)
}

/// Builds the `separators` codec parameter.
pub(crate) fn separators_param(separators: &[String]) -> ParamValue {
    ParamValue::List(
        separators
            .iter()
            .map(|separator| ParamValue::Str(separator.clone()))
            .collect(),
    )
}

/// The `PartKind` of the wrapper hint on a part node.
pub(crate) fn kind_from_hint(hint: &str) -> Option<PartKind> {
    match hint {
        "AC:part:json" => Some(PartKind::Json),
        "AC:part:xml" => Some(PartKind::Xml),
        "AC:part:nrbf" => Some(PartKind::Nrbf),
        _ => None,
    }
}

/// The value of a wrapper's `data` key.
pub(crate) fn data_child(
    doc: &SaveDoc,
    wrapper: saveprism_core::NodeId,
) -> Option<saveprism_core::NodeId> {
    match doc.get(wrapper).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries
            .iter()
            .find(|(key, _)| key == "data")
            .map(|(_, child)| *child),
        _ => None,
    }
}
