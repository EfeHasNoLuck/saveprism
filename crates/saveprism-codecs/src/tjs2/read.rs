//! Parser for the TJS2 `saveStruct` text produced by KiriKiri / KAG.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint,
    Value,
};

use super::{PREFIX, TJS_PARSE, depth_error};

/// Parses one TJS2 expression (an optional `(const)` before it) into `doc`.
pub(crate) fn parse(src: &str, doc: &mut SaveDoc, max_depth: usize) -> Result<NodeId> {
    let mut parser = Parser {
        src,
        at: 0,
        doc,
        max_depth,
    };
    parser.ws();
    let root = parser.value(0)?;
    parser.ws();
    if parser.at != src.len() {
        return Err(parser.error("unexpected text after the saved value"));
    }
    Ok(root)
}

struct Parser<'a, 'd> {
    src: &'a str,
    at: usize,
    doc: &'d mut SaveDoc,
    max_depth: usize,
}

impl Parser<'_, '_> {
    fn error(&self, message: &str) -> Error {
        Error::new(TJS_PARSE, format!("{message} at byte {}", self.at))
    }

    fn byte(&self) -> Option<u8> {
        self.src.as_bytes().get(self.at).copied()
    }

    fn rest(&self) -> &str {
        self.src.get(self.at..).unwrap_or_default()
    }

    /// Skips whitespace and `/* ... */` comments.
    fn ws(&mut self) {
        loop {
            while self
                .src
                .as_bytes()
                .get(self.at)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.at += 1;
            }
            if !self.rest().starts_with("/*") {
                return;
            }
            let body = self.rest().get(2..).unwrap_or_default();
            let Some(end) = body.find("*/") else {
                self.at = self.src.len();
                return;
            };
            self.at = self
                .at
                .saturating_add(2)
                .saturating_add(end)
                .saturating_add(2);
        }
    }

    /// Skips whitespace, an optional `(const)` prefix and whitespace again.
    fn prefix(&mut self) {
        self.ws();
        if self.rest().starts_with("(const)") {
            self.at = self.at.saturating_add(7);
            self.ws();
        }
    }

    fn enter(&self, depth: usize) -> Result<()> {
        if depth > self.max_depth {
            return Err(depth_error(depth));
        }
        Ok(())
    }

    fn push(&mut self, node: Node) -> Result<NodeId> {
        self.doc.push(node)
    }

    fn node(&self, value: Value, hint: &str, span: Option<(usize, usize)>) -> Node {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}{hint}")));
        node.span = span.map(|(start, end)| ByteSpan {
            start: len_u64(start),
            end: len_u64(end),
        });
        node
    }

    fn editable(&self, value: Value, hint: &str, span: Option<(usize, usize)>) -> Node {
        let mut node = self.node(value, hint, span);
        node.lexeme = Some("tjs2:original".into());
        node
    }

    fn read_only(&self, value: Value, hint: &str) -> Node {
        let mut node = self.node(value, hint, None);
        node.flags.insert(NodeFlags::READ_ONLY);
        node
    }

    fn value(&mut self, depth: usize) -> Result<NodeId> {
        self.prefix();
        let Some(byte) = self.byte() else {
            return Err(self.error("missing TJS2 value"));
        };
        match byte {
            b'%' if self
                .rest()
                .strip_prefix('%')
                .is_some_and(|rest| rest.trim_start().starts_with('[')) =>
            {
                self.dictionary(depth + 1)
            }
            b'[' => self.array(depth + 1),
            b'"' => {
                let start = self.at;
                let text = self.string_token()?;
                self.push(self.editable(Value::Str(text), "string", Some((start, self.at))))
            }
            b'<' if self.rest().starts_with("<%") => self.octet(),
            _ => self.scalar(),
        }
    }

    fn dictionary(&mut self, depth: usize) -> Result<NodeId> {
        self.enter(depth)?;
        self.at = self.at.saturating_add(1);
        self.ws();
        if self.byte() != Some(b'[') {
            return Err(self.error("expected `[` after `%`"));
        }
        self.at += 1;
        let mut entries: Vec<(String, NodeId)> = Vec::new();
        loop {
            self.ws();
            if self.byte() == Some(b']') {
                self.at += 1;
                break;
            }
            if self.at >= self.src.len() {
                return Err(self.error("unterminated TJS2 dictionary"));
            }
            if self.byte() != Some(b'"') {
                return Err(self.error("a TJS2 dictionary key is not a string literal"));
            }
            let key = self.string_token()?;
            self.ws();
            if !self.rest().starts_with("=>") {
                return Err(self.error("expected `=>` after a TJS2 dictionary key"));
            }
            self.at += 2;
            let value = self.value(depth)?;
            entries.push((key, value));
            self.ws();
            match self.byte() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected `,` or `]` in a TJS2 dictionary")),
            }
        }
        let node = self.read_only(Value::Object(entries), "dictionary");
        self.push(node)
    }

    fn array(&mut self, depth: usize) -> Result<NodeId> {
        self.enter(depth)?;
        self.at += 1;
        let mut items = Vec::new();
        loop {
            self.ws();
            if self.byte() == Some(b']') {
                self.at += 1;
                break;
            }
            if self.at >= self.src.len() {
                return Err(self.error("unterminated TJS2 array"));
            }
            items.push(self.value(depth)?);
            self.ws();
            match self.byte() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected `,` or `]` in a TJS2 array")),
            }
        }
        let node = self.read_only(Value::Array(items), "array");
        self.push(node)
    }

    fn octet(&mut self) -> Result<NodeId> {
        let start = self.at;
        self.at = self.at.saturating_add(2);
        let Some(close) = self.rest().find("%>") else {
            return Err(self.error("unterminated TJS2 octet literal"));
        };
        let body = self
            .rest()
            .get(..close)
            .ok_or_else(|| self.error("invalid TJS2 octet literal"))?;
        let mut bytes = Vec::new();
        for token in body.split_ascii_whitespace() {
            if token.len() != 2 {
                return Err(self.error("a TJS2 octet byte is not two hex digits"));
            }
            let byte = u8::from_str_radix(token, 16)
                .map_err(|_| self.error("a TJS2 octet byte is not two hex digits"))?;
            bytes.push(byte);
        }
        self.at = start
            .saturating_add(2)
            .saturating_add(close)
            .saturating_add(2);
        let node = self.read_only(Value::Bytes(bytes), "octet");
        self.push(node)
    }

    /// Reads a double-quoted string token at the current position (which must be `"`).
    fn string_token(&mut self) -> Result<String> {
        if self.byte() != Some(b'"') {
            return Err(self.error("expected a TJS2 string"));
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            let Some(character) = self.rest().chars().next() else {
                return Err(self.error("unterminated TJS2 string"));
            };
            self.at = self.at.saturating_add(character.len_utf8());
            match character {
                '"' => return Ok(out),
                '\\' => {
                    let Some(escape) = self.rest().chars().next() else {
                        return Err(self.error("unterminated TJS2 string escape"));
                    };
                    self.at = self.at.saturating_add(escape.len_utf8());
                    match escape {
                        'a' => out.push('\u{07}'),
                        'b' => out.push('\u{08}'),
                        'f' => out.push('\u{0c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'v' => out.push('\u{0b}'),
                        '\\' => out.push('\\'),
                        '\'' => out.push('\''),
                        '"' => out.push('"'),
                        '0' => out.push('\0'),
                        'x' => out.push(self.hex_escape()?),
                        _ => return Err(self.error("unsupported TJS2 string escape")),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn hex_escape(&mut self) -> Result<char> {
        let end = self.at.saturating_add(2);
        let text = self
            .src
            .get(self.at..end)
            .ok_or_else(|| self.error("truncated TJS2 \\x escape"))?;
        let code =
            u8::from_str_radix(text, 16).map_err(|_| self.error("invalid TJS2 \\x escape"))?;
        self.at = end;
        Ok(char::from(code))
    }

    fn scalar(&mut self) -> Result<NodeId> {
        let start = self.at;
        while let Some(byte) = self.byte() {
            if byte.is_ascii_whitespace() || matches!(byte, b',' | b']' | b'}' | b')' | b';' | b'/')
            {
                break;
            }
            self.at += 1;
        }
        let token = self
            .src
            .get(start..self.at)
            .ok_or_else(|| self.error("invalid TJS2 value"))?;
        match token {
            "void" => {
                let node = self.read_only(Value::Null, "void");
                return self.push(node);
            }
            "null" => {
                let node = self.read_only(Value::Null, "null");
                return self.push(node);
            }
            "nan" => {
                return self.push(self.editable(
                    Value::Float {
                        v: f64::NAN,
                        width: FloatWidth::F64,
                    },
                    "real",
                    Some((start, self.at)),
                ));
            }
            "inf" | "+inf" => {
                return self.push(self.editable(
                    Value::Float {
                        v: f64::INFINITY,
                        width: FloatWidth::F64,
                    },
                    "real",
                    Some((start, self.at)),
                ));
            }
            "-inf" => {
                return self.push(self.editable(
                    Value::Float {
                        v: f64::NEG_INFINITY,
                        width: FloatWidth::F64,
                    },
                    "real",
                    Some((start, self.at)),
                ));
            }
            _ => {}
        }
        if token.is_empty() {
            return Err(self.error("missing TJS2 value"));
        }
        if let Some(value) = parse_hex_real(token) {
            let end = self.real_comment_end();
            return self.push(self.editable(
                Value::Float {
                    v: value,
                    width: FloatWidth::F64,
                },
                "real",
                Some((start, end)),
            ));
        }
        let float = token.contains(['.', 'e', 'E']);
        if float {
            let value: f64 = token
                .parse()
                .map_err(|_| self.error("invalid TJS2 real number"))?;
            return self.push(self.editable(
                Value::Float {
                    v: value,
                    width: FloatWidth::F64,
                },
                "real",
                Some((start, self.at)),
            ));
        }
        let value = if let Some(hex) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            i128::from_str_radix(hex, 16).map_err(|_| self.error("invalid TJS2 integer"))?
        } else if let Some(hex) = token
            .strip_prefix("-0x")
            .or_else(|| token.strip_prefix("-0X"))
        {
            i128::from_str_radix(hex, 16)
                .map_err(|_| self.error("invalid TJS2 integer"))?
                .checked_neg()
                .ok_or_else(|| self.error("TJS2 integer is out of range"))?
        } else {
            token
                .parse::<i128>()
                .map_err(|_| self.error("invalid TJS2 integer"))?
        };
        self.push(self.editable(
            Value::Int {
                v: value,
                width: IntWidth::I64,
            },
            "int",
            Some((start, self.at)),
        ))
    }

    /// A real written by `TJSVariantToExpressionString` carries ` /* decimal */`; include it in
    /// the span so an edit replaces the whole token.
    fn real_comment_end(&self) -> usize {
        let mut probe = self.at;
        while self
            .src
            .as_bytes()
            .get(probe)
            .is_some_and(u8::is_ascii_whitespace)
        {
            probe += 1;
        }
        let Some(rest) = self.src.get(probe..) else {
            return self.at;
        };
        let Some(body) = rest.strip_prefix("/*") else {
            return self.at;
        };
        match body.find("*/") {
            Some(end) => probe
                .saturating_add(2)
                .saturating_add(end)
                .saturating_add(2),
            None => self.at,
        }
    }
}

/// Parses the hexadecimal float TJS writes for a real (`0x1.8p+1`, `-0x1.0p-2`).
fn parse_hex_real(token: &str) -> Option<f64> {
    let (negative, unsigned) = match token.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, token.strip_prefix('+').unwrap_or(token)),
    };
    let rest = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))?;
    let split = rest.find(['p', 'P'])?;
    let mantissa = rest.get(..split)?;
    let exponent: i32 = rest.get(split + 1..)?.parse().ok()?;
    let (integer, fraction) = match mantissa.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (mantissa, None),
    };
    let mut value = integer.parse::<u64>().ok()? as f64;
    if let Some(fraction) = fraction {
        let mut scale = 1.0_f64;
        for character in fraction.chars() {
            let digit = character.to_digit(16)?;
            scale /= 16.0;
            value += f64::from(digit) * scale;
        }
    }
    let value = value * 2.0_f64.powi(exponent);
    Some(if negative { -value } else { value })
}
