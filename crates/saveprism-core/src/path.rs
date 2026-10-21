//! JSON Pointer (RFC 6901) paths that address nodes of a [`SaveDoc`](crate::SaveDoc).
//!
//! Resolution depends on the container kind: `Object` matches a key (the first
//! match when a key repeats), `Array` matches an index, `Map` matches an entry
//! index and resolves to the entry's **value** node.

use std::fmt;

use crate::error::{Error, Result, codes};

/// A parsed JSON Pointer: a list of unescaped segments. The empty path is the root.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Path(Vec<String>);

impl Path {
    /// The root path (`""`).
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Parses a JSON Pointer such as `""`, `"/a/0"` or `"/a~1b/~0c"`.
    pub fn parse(text: &str) -> Result<Path> {
        if text.is_empty() {
            return Ok(Path::root());
        }
        let rest = text.strip_prefix('/').ok_or_else(|| {
            Error::new(
                codes::CORE_INVALID_PATH,
                format!("path '{text}' must be empty or start with '/'"),
            )
        })?;
        let mut segments = Vec::new();
        for raw in rest.split('/') {
            segments.push(unescape(raw).ok_or_else(|| {
                Error::new(
                    codes::CORE_INVALID_PATH,
                    format!("path '{text}' contains an invalid '~' escape (use ~0 or ~1)"),
                )
            })?);
        }
        Ok(Path(segments))
    }

    /// Appends one unescaped segment.
    pub fn push(&mut self, segment: impl Into<String>) {
        self.0.push(segment.into());
    }

    /// The unescaped segments from the root downwards.
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// True for the root path.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The path without its last segment, or `None` for the root.
    pub fn parent(&self) -> Option<Path> {
        self.0.split_last().map(|(_, rest)| Path(rest.to_vec()))
    }

    /// The last segment, or `None` for the root.
    pub fn last(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.0 {
            f.write_str("/")?;
            f.write_str(&segment.replace('~', "~0").replace('/', "~1"))?;
        }
        Ok(())
    }
}

/// Reverses the `~0` / `~1` escapes; `None` for a dangling or unknown escape.
fn unescape(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            match chars.next() {
                Some('0') => out.push('~'),
                Some('1') => out.push('/'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// Parses an array index segment: ASCII digits only, no leading zeros.
pub(crate) fn parse_index(segment: &str) -> Option<usize> {
    if segment.is_empty() || !segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if segment.len() > 1 && segment.starts_with('0') {
        return None;
    }
    segment.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_root() {
        let path = Path::parse("").unwrap();
        assert!(path.is_root());
        assert_eq!(path.to_string(), "");
        assert!(path.parent().is_none());
    }

    #[test]
    fn escapes_are_decoded_in_order() {
        let path = Path::parse("/a/~1b/~0c/~01").unwrap();
        assert_eq!(path.segments(), ["a", "/b", "~c", "~1"]);
    }

    #[test]
    fn display_round_trips() {
        for text in ["", "/a", "/a/0/b", "/a~1b/~0c", "//", "/~01"] {
            let path = Path::parse(text).unwrap();
            assert_eq!(path.to_string(), text);
        }
    }

    #[test]
    fn empty_segment_is_allowed() {
        let path = Path::parse("/").unwrap();
        assert_eq!(path.segments(), [""]);
    }

    #[test]
    fn invalid_paths_are_rejected() {
        for text in ["a", "a/b", "/a~", "/a~2", "/~"] {
            let error = Path::parse(text).unwrap_err();
            assert_eq!(error.code(), codes::CORE_INVALID_PATH, "{text}");
        }
    }

    #[test]
    fn push_parent_last() {
        let mut path = Path::root();
        path.push("a");
        path.push("b/c");
        assert_eq!(path.last(), Some("b/c"));
        assert_eq!(path.parent().unwrap().to_string(), "/a");
        assert_eq!(path.to_string(), "/a/b~1c");
    }

    #[test]
    fn index_parsing_is_strict() {
        assert_eq!(parse_index("0"), Some(0));
        assert_eq!(parse_index("12"), Some(12));
        assert_eq!(parse_index("01"), None);
        assert_eq!(parse_index("+1"), None);
        assert_eq!(parse_index("-"), None);
        assert_eq!(parse_index(""), None);
        assert_eq!(parse_index("99999999999999999999999999"), None);
    }
}
