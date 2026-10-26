//! Iterative JSON parser: builds a `SaveDoc` with an explicit stack (no recursion).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc,
    TypeHint, Value,
};

use super::profile::Observed;
use super::write::{float_text, write_string};
use super::{JSON_LONE_SURROGATE, JSON_SYNTAX, JSON_TRAILING};

/// A parsed document plus the whitespace observations the profile is inferred from.
pub(crate) struct Parsed {
    pub doc: SaveDoc,
    pub observed: Observed,
}

/// Which object keys the parser accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyStyle {
    /// Strict JSON: a key is always a quoted string.
    Strict,
    /// Easy Save 3: a key may also be a bare number or `true`/`false` (dictionaries whose key
    /// type is not a string).
    Bare,
}

/// An open container while its children are being read.
enum Frame {
    Array {
        items: Vec<NodeId>,
        start: usize,
    },
    Object {
        entries: Vec<(String, NodeId)>,
        /// For every entry, whether its key was written bare.
        bare: Vec<bool>,
        /// Key of the value being read.
        key: String,
        start: usize,
    },
    /// An Easy Save 3 dictionary whose key type is a struct or array: the key is itself an
    /// object or array, so the entries are kept as [`Value::Map`] pairs.
    Map {
        entries: Vec<(NodeId, NodeId)>,
        /// The key of the value being read, once its node is complete.
        key: Option<NodeId>,
        start: usize,
    },
}

struct StrToken {
    value: String,
    lone_surrogate: bool,
    /// The source token differs from the canonical writing of the value.
    needs_lexeme: bool,
}

struct Parser<'a, 'c> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// Offset of `text` in the original input (the BOM length).
    base: u64,
    ctx: &'a DecodeCtx<'c>,
    style: KeyStyle,
    doc: SaveDoc,
    obs: Observed,
}

/// Parses `text` (BOM already removed; `base` is the BOM length for offsets).
pub(crate) fn parse(text: &str, base: u64, ctx: &DecodeCtx<'_>, style: KeyStyle) -> Result<Parsed> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
        base,
        ctx,
        style,
        doc: ctx.new_doc(),
        obs: Observed::default(),
    };
    parser.skip_ws();
    let root = parser.parse_tree()?;
    let tail_start = parser.pos;
    parser.skip_ws();
    parser.obs.tail = tail_start..parser.pos;
    if parser.pos < parser.bytes.len() {
        return Err(Error::new(
            JSON_TRAILING,
            "unexpected data after the end of the JSON value",
        )
        .with_offset(parser.abs(parser.pos)));
    }
    parser.doc.set_root(root);
    Ok(Parsed {
        doc: parser.doc,
        observed: parser.obs,
    })
}

fn parse_hex(digits: &[u8]) -> Option<u32> {
    digits.iter().try_fold(0_u32, |acc, byte| {
        char::from(*byte).to_digit(16).map(|digit| acc * 16 + digit)
    })
}

impl Parser<'_, '_> {
    fn abs(&self, pos: usize) -> u64 {
        self.base.saturating_add(len_u64(pos))
    }

    fn syntax(&self, pos: usize, message: impl Into<String>) -> Error {
        Error::new(JSON_SYNTAX, message).with_offset(self.abs(pos))
    }

    /// Error for the character (or the end of the text) at the current position.
    fn unexpected(&self) -> Error {
        match self.text.get(self.pos..) {
            Some(rest) => match rest.chars().next() {
                Some(found) => self.syntax(self.pos, format!("unexpected character {found:?}")),
                None => self.syntax(
                    self.pos,
                    "unexpected end of the text (the JSON is probably truncated)",
                ),
            },
            None => self.syntax(self.pos, "unexpected position inside a character"),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) -> std::ops::Range<usize> {
        let start = self.pos;
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
        start..self.pos
    }

    fn note_open(&mut self, ws: std::ops::Range<usize>) {
        if self.obs.after_open.is_none() {
            self.obs.after_open = Some(ws);
        }
    }

    fn note_comma(&mut self, ws: std::ops::Range<usize>) {
        if self.obs.after_comma.is_none() {
            self.obs.after_comma = Some(ws);
        }
    }

    fn note_empty(&mut self, ws: std::ops::Range<usize>) {
        if self.obs.empty_interior.is_none() {
            self.obs.empty_interior = Some(ws);
        }
    }

    fn push(&mut self, mut node: Node, start: usize) -> Result<NodeId> {
        node.span = Some(ByteSpan {
            start: self.abs(start),
            end: self.abs(self.pos),
        });
        self.doc
            .push(node)
            .map_err(|error| error.with_offset(self.abs(start)))
    }

    fn open_depth(&self, depth: usize) -> Result<()> {
        self.ctx
            .check_depth(depth)
            .map_err(|error| error.with_offset(self.abs(self.pos)))
    }

    /// Reads one value (any nesting) starting at the current position.
    fn parse_tree(&mut self) -> Result<NodeId> {
        let mut stack: Vec<Frame> = Vec::new();
        loop {
            let mut done = match self.peek() {
                Some(b'[') => {
                    self.open_depth(stack.len() + 1)?;
                    let start = self.pos;
                    self.pos += 1;
                    let ws = self.skip_ws();
                    if self.peek() == Some(b']') {
                        self.pos += 1;
                        self.note_empty(ws);
                        self.push(Node::new(Value::Array(Vec::new())), start)?
                    } else {
                        self.note_open(ws);
                        stack.push(Frame::Array {
                            items: Vec::new(),
                            start,
                        });
                        continue;
                    }
                }
                Some(b'{') => {
                    self.open_depth(stack.len() + 1)?;
                    let start = self.pos;
                    self.pos += 1;
                    let ws = self.skip_ws();
                    if self.peek() == Some(b'}') {
                        self.pos += 1;
                        self.note_empty(ws);
                        self.push(Node::new(Value::Object(Vec::new())), start)?
                    } else {
                        self.note_open(ws);
                        if self.style == KeyStyle::Bare && matches!(self.peek(), Some(b'{' | b'['))
                        {
                            stack.push(Frame::Map {
                                entries: Vec::new(),
                                key: None,
                                start,
                            });
                            continue;
                        }
                        let (key, bare) = self.parse_key()?;
                        stack.push(Frame::Object {
                            entries: Vec::new(),
                            bare: vec![bare],
                            key,
                            start,
                        });
                        continue;
                    }
                }
                Some(_) => self.parse_scalar()?,
                None => return Err(self.unexpected()),
            };

            // A value is complete: attach it to its container, closing containers as needed.
            loop {
                match stack.last_mut() {
                    None => return Ok(done),
                    Some(Frame::Array { items, start }) => {
                        items.push(done);
                        self.skip_ws();
                        match self.peek() {
                            Some(b',') => {
                                self.pos += 1;
                                let ws = self.skip_ws();
                                self.note_comma(ws);
                                break;
                            }
                            Some(b']') => {
                                self.pos += 1;
                                let items = std::mem::take(items);
                                let start = *start;
                                stack.pop();
                                done = self.push(Node::new(Value::Array(items)), start)?;
                            }
                            _ => return Err(self.unexpected()),
                        }
                    }
                    Some(Frame::Object {
                        entries,
                        bare,
                        key,
                        start,
                    }) => {
                        entries.push((std::mem::take(key), done));
                        self.skip_ws();
                        match self.peek() {
                            Some(b',') => {
                                self.pos += 1;
                                let ws = self.skip_ws();
                                self.note_comma(ws);
                                let (next_key, next_bare) = self.parse_key()?;
                                *key = next_key;
                                bare.push(next_bare);
                                break;
                            }
                            Some(b'}') => {
                                self.pos += 1;
                                let entries = std::mem::take(entries);
                                let bare = std::mem::take(bare);
                                let start = *start;
                                stack.pop();
                                let mut node = Node::new(Value::Object(entries));
                                node.lexeme = super::bare_key_lexeme(&bare);
                                done = self.push(node, start)?;
                            }
                            _ => return Err(self.unexpected()),
                        }
                    }
                    Some(Frame::Map {
                        entries,
                        key,
                        start,
                    }) => {
                        if let Some(key_id) = key.take() {
                            entries.push((key_id, done));
                            self.skip_ws();
                            match self.peek() {
                                Some(b',') => {
                                    self.pos += 1;
                                    let ws = self.skip_ws();
                                    self.note_comma(ws);
                                    break;
                                }
                                Some(b'}') => {
                                    self.pos += 1;
                                    let entries = std::mem::take(entries);
                                    let start = *start;
                                    stack.pop();
                                    done = self.push(Node::new(Value::Map(entries)), start)?;
                                }
                                _ => return Err(self.unexpected()),
                            }
                        } else {
                            // `done` is the structured key; the colon follows it.
                            let before = self.skip_ws();
                            if self.obs.before_colon.is_none() {
                                self.obs.before_colon = Some(before);
                            }
                            if self.peek() != Some(b':') {
                                return Err(self.unexpected());
                            }
                            self.pos += 1;
                            let ws = self.skip_ws();
                            if self.obs.after_colon_bare.is_none() {
                                self.obs.after_colon_bare = Some(ws);
                            }
                            *key = Some(done);
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Reads a key (a quoted string, or in [`KeyStyle::Bare`] a number or `true`/`false`), the
    /// optional whitespace and the colon, then the whitespace after the colon. Returns the key
    /// text and whether it was written bare.
    fn parse_key(&mut self) -> Result<(String, bool)> {
        let (value, bare) = if self.peek() == Some(b'"') {
            let start = self.pos;
            let token = self.parse_string()?;
            if token.lone_surrogate {
                return Err(Error::new(
                    JSON_LONE_SURROGATE,
                    "an object key contains an unpaired surrogate escape; the key cannot be kept without loss",
                )
                .with_offset(self.abs(start)));
            }
            (token.value, false)
        } else if self.style == KeyStyle::Bare {
            self.parse_bare_key()?
        } else {
            return Err(self.unexpected());
        };
        let before = self.skip_ws();
        if self.obs.before_colon.is_none() {
            self.obs.before_colon = Some(before);
        }
        if self.peek() != Some(b':') {
            return Err(self.unexpected());
        }
        self.pos += 1;
        let ws = self.skip_ws();
        let observed = if bare {
            &mut self.obs.after_colon_bare
        } else {
            &mut self.obs.after_colon
        };
        if observed.is_none() {
            *observed = Some(ws);
        }
        Ok((value, bare))
    }

    /// Reads a bare dictionary key: a JSON number token or `true`/`false`, keeping its exact text.
    fn parse_bare_key(&mut self) -> Result<(String, bool)> {
        let start = self.pos;
        match self.peek() {
            Some(b'-' | b'0'..=b'9') => {
                self.scan_number()?;
            }
            Some(b't') => self.literal("true")?,
            Some(b'f') => self.literal("false")?,
            _ => return Err(self.unexpected()),
        }
        let text = self
            .text
            .get(start..self.pos)
            .ok_or_else(|| self.syntax(start, "invalid dictionary key"))?;
        Ok((text.to_string(), true))
    }

    fn parse_scalar(&mut self) -> Result<NodeId> {
        let start = self.pos;
        match self.peek() {
            Some(b'"') => {
                let token = self.parse_string()?;
                let mut node = Node::new(Value::Str(token.value));
                if token.needs_lexeme {
                    node.lexeme = self.text.get(start..self.pos).map(Into::into);
                }
                if token.lone_surrogate {
                    node.flags.insert(NodeFlags::READ_ONLY);
                }
                self.push(node, start)
            }
            Some(b'n') => {
                self.literal("null")?;
                self.push(Node::new(Value::Null), start)
            }
            Some(b't') => {
                self.literal("true")?;
                self.push(Node::new(Value::Bool(true)), start)
            }
            Some(b'f') => {
                self.literal("false")?;
                self.push(Node::new(Value::Bool(false)), start)
            }
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            _ => Err(self.unexpected()),
        }
    }

    fn literal(&mut self, word: &str) -> Result<()> {
        let matches = self
            .bytes
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(word.as_bytes()));
        if matches {
            self.pos += word.len();
            Ok(())
        } else {
            Err(self.syntax(self.pos, format!("invalid literal, expected `{word}`")))
        }
    }

    fn skip_digits(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos > start
    }

    /// Advances over a JSON number token at the current position and reports whether it is whole
    /// (no fraction and no exponent).
    fn scan_number(&mut self) -> Result<bool> {
        let mut whole = true;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                self.skip_digits();
            }
            _ => return Err(self.unexpected()),
        }
        if self.peek() == Some(b'.') {
            whole = false;
            self.pos += 1;
            if !self.skip_digits() {
                return Err(self.unexpected());
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            whole = false;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.skip_digits() {
                return Err(self.unexpected());
            }
        }
        Ok(whole)
    }

    fn parse_number(&mut self) -> Result<NodeId> {
        let start = self.pos;
        let whole = self.scan_number()?;
        let token = self
            .text
            .get(start..self.pos)
            .ok_or_else(|| self.syntax(start, "invalid number"))?;

        let whole_value = if whole {
            token.parse::<i128>().ok()
        } else {
            None
        };
        if let Some(v) = whole_value {
            let mut node = Node::new(Value::Int {
                v,
                width: IntWidth::Unbounded,
            });
            node.type_hint = Some(TypeHint::new("JSON:int"));
            // The only whole number whose text differs from `v.to_string()` is "-0".
            if token == "-0" {
                node.lexeme = Some(token.into());
            }
            return self.push(node, start);
        }

        let v = token
            .parse::<f64>()
            .map_err(|_| self.syntax(start, "invalid number"))?;
        let mut node = Node::new(Value::Float {
            v,
            width: FloatWidth::F64,
        });
        node.type_hint = Some(TypeHint::new("JSON:float"));
        if float_text(v, true).as_deref() != Some(token) {
            node.lexeme = Some(token.into());
        }
        self.push(node, start)
    }

    /// Reads a string token; the current position is on the opening quote.
    fn parse_string(&mut self) -> Result<StrToken> {
        let start = self.pos;
        self.pos += 1;
        let mut value = String::new();
        let mut run = self.pos;
        let mut has_escape = false;
        let mut lone_surrogate = false;
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.syntax(
                    self.pos,
                    "unterminated string (the JSON is probably truncated)",
                ));
            };
            match byte {
                b'"' => {
                    value.push_str(self.text.get(run..self.pos).unwrap_or_default());
                    self.pos += 1;
                    break;
                }
                b'\\' => {
                    value.push_str(self.text.get(run..self.pos).unwrap_or_default());
                    has_escape = true;
                    self.pos += 1;
                    self.parse_escape(&mut value, &mut lone_surrogate)?;
                    run = self.pos;
                }
                0x00..=0x1F => {
                    return Err(self.syntax(
                        self.pos,
                        "control character inside a string (it must be escaped)",
                    ));
                }
                _ => self.pos += 1,
            }
        }

        let needs_lexeme = has_escape
            && (lone_surrogate || {
                let mut canonical = Vec::new();
                write_string(&mut canonical, &value);
                self.text.get(start..self.pos).map(str::as_bytes) != Some(canonical.as_slice())
            });
        Ok(StrToken {
            value,
            lone_surrogate,
            needs_lexeme,
        })
    }

    /// Reads the escape after a backslash (already consumed) and appends its character.
    fn parse_escape(&mut self, value: &mut String, lone_surrogate: &mut bool) -> Result<()> {
        let Some(escape) = self.peek() else {
            return Err(self.syntax(
                self.pos,
                "unterminated string (the JSON is probably truncated)",
            ));
        };
        self.pos += 1;
        match escape {
            b'"' => value.push('"'),
            b'\\' => value.push('\\'),
            b'/' => value.push('/'),
            b'b' => value.push('\u{8}'),
            b'f' => value.push('\u{c}'),
            b'n' => value.push('\n'),
            b'r' => value.push('\r'),
            b't' => value.push('\t'),
            b'u' => {
                let unit = self.hex4()?;
                match unit {
                    0xD800..=0xDBFF => match self.low_surrogate_ahead() {
                        Some(low) => {
                            self.pos += 6;
                            let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                            value.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        None => {
                            *lone_surrogate = true;
                            value.push('\u{FFFD}');
                        }
                    },
                    0xDC00..=0xDFFF => {
                        *lone_surrogate = true;
                        value.push('\u{FFFD}');
                    }
                    _ => value.push(char::from_u32(unit).unwrap_or('\u{FFFD}')),
                }
            }
            _ => {
                return Err(self.syntax(self.pos - 1, "invalid escape sequence in a string"));
            }
        }
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32> {
        let unit = self.bytes.get(self.pos..self.pos + 4).and_then(parse_hex);
        match unit {
            Some(unit) => {
                self.pos += 4;
                Ok(unit)
            }
            None => Err(self.syntax(
                self.pos,
                "invalid \\u escape: four hexadecimal digits expected",
            )),
        }
    }

    /// The low surrogate of a `\uXXXX\uXXXX` pair starting at the current position, if any.
    fn low_surrogate_ahead(&self) -> Option<u32> {
        let rest = self.bytes.get(self.pos..)?;
        if !rest.starts_with(b"\\u") {
            return None;
        }
        parse_hex(rest.get(2..6)?).filter(|unit| (0xDC00..=0xDFFF).contains(unit))
    }
}
