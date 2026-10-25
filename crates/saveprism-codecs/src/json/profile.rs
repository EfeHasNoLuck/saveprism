//! Format profile of a JSON text: indentation, separators, line endings, BOM.
//!
//! The profile is inferred from the whitespace the parser saw and stored in the step `Params`.
//! `decode` then writes the document again with the profile and compares it with the source; when
//! they differ the document is marked `regular = false` ("semantically equal" in the round-trip guard).

use std::ops::Range;

use saveprism_core::{Error, ParamValue, Params, Result, codes};

use crate::text_encoding::TextEncoding;

/// Largest indent (spaces per level) the profile accepts.
const MAX_INDENT: usize = 32;

/// Indentation of nested containers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Indent {
    /// Everything on one line.
    None,
    /// One line per element, this many spaces per level.
    Spaces(usize),
    /// One line per element, one tab per level.
    Tab,
}

/// How a JSON text is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    pub indent: Indent,
    /// Line ending is `\r\n` instead of `\n`.
    pub crlf: bool,
    /// Compact mode only: `", "` instead of `","`.
    pub comma_space: bool,
    /// `": "` instead of `":"`.
    pub colon_space: bool,
    /// Empty containers are written `[ ]` / `{ }` instead of `[]` / `{}`.
    pub empty_spaced: bool,
    pub trailing_newline: bool,
    /// The file starts with the BOM of `text_encoding`.
    pub bom: bool,
    /// Encoding of the file (UTF-8 unless detection found another one).
    pub text_encoding: TextEncoding,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            indent: Indent::None,
            crlf: false,
            comma_space: false,
            colon_space: false,
            empty_spaced: false,
            trailing_newline: false,
            bom: false,
            text_encoding: TextEncoding::Utf8,
        }
    }
}

/// Whitespace ranges (into the BOM-less text) that the parser observed.
#[derive(Debug, Default)]
pub(crate) struct Observed {
    /// Whitespace right after the first non-empty `[` or `{`.
    pub after_open: Option<Range<usize>>,
    /// Whitespace right after the first `,`.
    pub after_comma: Option<Range<usize>>,
    /// Whitespace right after the first `:`.
    pub after_colon: Option<Range<usize>>,
    /// Whitespace inside the first empty container.
    pub empty_interior: Option<Range<usize>>,
    /// Whitespace after the root value.
    pub tail: Range<usize>,
}

/// The observed whitespace, or "" when nothing was observed.
fn ws_text<'t>(text: &'t str, range: &Option<Range<usize>>) -> &'t str {
    range
        .as_ref()
        .and_then(|r| text.get(r.clone()))
        .unwrap_or("")
}

fn invalid(key: &str) -> Error {
    Error::new(
        codes::CORE_INVALID_PARAM,
        format!("parameter \"{key}\" has a value the JSON codec does not support"),
    )
}

impl Profile {
    /// Infers the profile from the observed whitespace. Guesses may be wrong for hand-edited
    /// text; `decode` verifies the guess by writing the document again.
    pub(crate) fn infer(text: &str, observed: &Observed, bom: bool) -> Self {
        let slice = |range: &Option<Range<usize>>| ws_text(text, range);
        let mut profile = Profile {
            bom,
            ..Profile::default()
        };

        let open = slice(&observed.after_open);
        if let Some(newline) = open.find('\n') {
            profile.crlf = open
                .get(..newline)
                .is_some_and(|before| before.ends_with('\r'));
            let rest = open.get(newline + 1..).unwrap_or("");
            profile.indent = if rest == "\t" {
                Indent::Tab
            } else if rest.bytes().all(|b| b == b' ') {
                Indent::Spaces(rest.len().min(MAX_INDENT))
            } else {
                Indent::Spaces(2)
            };
        }
        if profile.indent == Indent::None {
            profile.comma_space = slice(&observed.after_comma) == " ";
        }
        profile.colon_space = slice(&observed.after_colon) == " ";
        profile.empty_spaced = slice(&observed.empty_interior) == " ";

        match text.get(observed.tail.clone()).unwrap_or("") {
            "\n" => profile.trailing_newline = true,
            "\r\n" => {
                profile.trailing_newline = true;
                if profile.indent == Indent::None {
                    profile.crlf = true;
                }
            }
            _ => {}
        }
        profile
    }

    /// The profile as step parameters. `regular` tells whether writing with this profile
    /// reproduces the source bytes.
    pub(crate) fn to_params(self, regular: bool) -> Params {
        let mut params = Params::new();
        match self.indent {
            Indent::None => params.set("indent", "none"),
            Indent::Spaces(count) => params.set("indent", i64::try_from(count).unwrap_or(2)),
            Indent::Tab => params.set("indent", "tab"),
        }
        params.set("newline", if self.crlf { "crlf" } else { "lf" });
        params.set("item_sep", if self.comma_space { ", " } else { "," });
        params.set("key_sep", if self.colon_space { ": " } else { ":" });
        params.set("empty", if self.empty_spaced { "[ ]" } else { "[]" });
        params.set("trailing_newline", self.trailing_newline);
        params.set("bom", self.bom);
        params.set(super::P_TEXT_ENCODING, self.text_encoding.name());
        params.set("regular", regular);
        params
    }

    /// Reads a profile from step parameters; absent keys take the compact defaults.
    pub(crate) fn from_params(params: &Params) -> Result<Self> {
        let indent = match params.get("indent") {
            None => Indent::None,
            Some(ParamValue::Str(text)) if text == "none" => Indent::None,
            Some(ParamValue::Str(text)) if text == "tab" => Indent::Tab,
            Some(ParamValue::Int(count)) => match usize::try_from(*count) {
                Ok(count) if count <= MAX_INDENT => Indent::Spaces(count),
                _ => return Err(invalid("indent")),
            },
            Some(_) => return Err(invalid("indent")),
        };
        let text_encoding = match params.opt_str(super::P_TEXT_ENCODING)? {
            None => TextEncoding::Utf8,
            Some(name) => TextEncoding::from_name(name).ok_or_else(|| invalid("text_encoding"))?,
        };
        let crlf = match params.opt_str("newline")? {
            None | Some("lf") => false,
            Some("crlf") => true,
            Some(_) => return Err(invalid("newline")),
        };
        let comma_space = match params.opt_str("item_sep")? {
            None | Some(",") => false,
            Some(", ") => true,
            Some(_) => return Err(invalid("item_sep")),
        };
        let colon_space = match params.opt_str("key_sep")? {
            None | Some(":") => false,
            Some(": ") => true,
            Some(_) => return Err(invalid("key_sep")),
        };
        let empty_spaced = match params.opt_str("empty")? {
            None | Some("[]") => false,
            Some("[ ]") => true,
            Some(_) => return Err(invalid("empty")),
        };
        Ok(Self {
            indent,
            crlf,
            comma_space,
            colon_space,
            empty_spaced,
            trailing_newline: params.opt_bool("trailing_newline")?.unwrap_or(false),
            bom: params.opt_bool("bom")?.unwrap_or(false),
            text_encoding,
        })
    }
}
