//! The text `SiiNunit` reader (see `mod.rs` for the layout and the tree).

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Limits, Node, NodeFlags, NodeId, Params, Result,
    SaveDoc, TypeHint, Value,
};

use super::{HINT_PREFIX, ORIGINAL, SCS_SII_PARSE};

fn parse_error(message: impl Into<String>, at: usize) -> Error {
    Error::new(SCS_SII_PARSE, message).with_offset(len_u64(at))
}

fn hint(name: &str) -> TypeHint {
    TypeHint::new(format!("{HINT_PREFIX}{name}"))
}

/// Decodes a plain-text `SiiNunit` file.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<(SaveDoc, Params)> {
    let text =
        std::str::from_utf8(input).map_err(|_| parse_error("the SII file is not UTF-8 text", 0))?;
    let mut parser = Parser {
        input: text,
        pos: if text.starts_with('\u{feff}') { 3 } else { 0 },
        doc: SaveDoc::with_node_limit(limits.max_nodes),
    };
    parser.skip();
    if !parser.starts_with("SiiNunit") {
        return Err(parse_error(
            "the SII file does not start with SiiNunit",
            parser.pos,
        ));
    }
    parser.pos += "SiiNunit".len();
    parser.skip();
    parser.expect('{')?;
    let mut units = Vec::new();
    loop {
        parser.skip();
        if parser.eat('}') {
            break;
        }
        if parser.pos >= parser.input.len() {
            return Err(parse_error(
                "the SII file ends before its closing brace",
                parser.pos,
            ));
        }
        let unit = parser.unit()?;
        units.push(unit);
    }
    parser.skip();
    if parser.pos != parser.input.len() {
        return Err(parse_error(
            "text follows the closing brace of the SII file",
            parser.pos,
        ));
    }
    let mut root = Node::new(Value::Object(units));
    root.type_hint = Some(hint("file"));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = parser.doc.push(root)?;
    parser.doc.set_root(root);
    let mut params = Params::new();
    params.set(ORIGINAL, input.to_vec());
    Ok((parser.doc, params))
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    doc: SaveDoc,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        self.input.get(self.pos..).unwrap_or_default()
    }

    fn byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn starts_with(&self, text: &str) -> bool {
        self.rest().starts_with(text)
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.rest().starts_with(ch) {
            self.pos += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, ch: char) -> Result<()> {
        if self.eat(ch) {
            Ok(())
        } else {
            Err(parse_error(format!("expected '{ch}'"), self.pos))
        }
    }

    /// Skips whitespace, `#`/`//` line comments and `/* ... */` block comments.
    fn skip(&mut self) {
        loop {
            let before = self.pos;
            while self.byte().is_some_and(|b| b.is_ascii_whitespace()) {
                self.pos += 1;
            }
            if self.starts_with("//") || self.starts_with("#") {
                self.pos += if self.starts_with("//") { 2 } else { 1 };
                while self.byte().is_some_and(|b| b != b'\n') {
                    self.pos += 1;
                }
                continue;
            }
            if self.starts_with("/*") {
                self.pos += 2;
                if let Some(close) = self.rest().find("*/") {
                    self.pos += close + 2;
                } else {
                    self.pos = self.input.len();
                }
                continue;
            }
            if self.pos == before {
                break;
            }
        }
    }

    /// Reads a name up to `:` (or whitespace when `stop_at_space`).
    fn word(&mut self, stop_at_space: bool) -> &str {
        let start = self.pos;
        while let Some(byte) = self.byte() {
            if byte == b':' || byte == b'{' || byte == b'}' {
                break;
            }
            if stop_at_space && byte.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
        self.input.get(start..self.pos).unwrap_or_default().trim()
    }

    fn unit(&mut self) -> Result<(String, NodeId)> {
        let class = self.word(false).to_owned();
        if class.is_empty() {
            return Err(parse_error("a unit has no class name", self.pos));
        }
        self.skip();
        self.expect(':')?;
        self.skip();
        let name_start = self.pos;
        let name = self.word(true).to_owned();
        if name.is_empty() {
            return Err(parse_error("a unit has no name", name_start));
        }
        self.skip();
        self.expect('{')?;
        let mut attributes = Vec::new();
        loop {
            self.skip();
            if self.eat('}') {
                break;
            }
            if self.pos >= self.input.len() {
                return Err(parse_error(
                    "the unit ends before its closing brace",
                    self.pos,
                ));
            }
            if self.starts_with("@include") {
                while self.byte().is_some_and(|b| b != b'\n') {
                    self.pos += 1;
                }
                continue;
            }
            let name_start = self.pos;
            let attribute = self.word(false).to_owned();
            if attribute.is_empty() {
                return Err(parse_error("an attribute has no name", name_start));
            }
            self.skip();
            self.expect(':')?;
            let value = self.value()?;
            let name_node =
                self.push(Value::Str(attribute), "name", name_start..name_start, true)?;
            let mut entry = Node::new(Value::Object(vec![
                ("name".to_owned(), name_node),
                ("value".to_owned(), value),
            ]));
            entry.type_hint = Some(hint("attribute"));
            entry.flags.insert(NodeFlags::READ_ONLY);
            let entry = self.doc.push(entry)?;
            attributes.push(entry);
        }
        let attributes = self.push(Value::Array(attributes), "attributes", 0..0, true)?;
        let mut class_node = Node::new(Value::Str(class));
        class_node.type_hint = Some(hint("class"));
        class_node.flags.insert(NodeFlags::READ_ONLY);
        let class_node = self.doc.push(class_node)?;
        let mut unit = Node::new(Value::Object(vec![
            ("class".to_owned(), class_node),
            ("attributes".to_owned(), attributes),
        ]));
        unit.type_hint = Some(hint("unit"));
        unit.flags.insert(NodeFlags::READ_ONLY);
        let unit = self.doc.push(unit)?;
        Ok((name, unit))
    }

    fn push(
        &mut self,
        value: Value,
        hint_name: &str,
        span: std::ops::Range<usize>,
        read_only: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(hint(hint_name));
        if span.start != span.end {
            node.span = Some(ByteSpan {
                start: len_u64(span.start),
                end: len_u64(span.end),
            });
        }
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        } else {
            node.lexeme = Some("sii:original".into());
        }
        self.doc.push(node)
    }

    /// Reads one attribute value from just after its `:` to the end of the line.
    fn value(&mut self) -> Result<NodeId> {
        while self.byte().is_some_and(|b| b == b' ' || b == b'\t') {
            self.pos += 1;
        }
        let start = self.pos;
        let mut end = self.pos;
        let mut quote = false;
        let mut escaped = false;
        let mut depth = 0_usize;
        while let Some(byte) = self.byte() {
            if quote {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quote = false;
                }
                end += 1;
                self.pos += 1;
                continue;
            }
            match byte {
                b'"' => {
                    quote = true;
                    end += 1;
                    self.pos += 1;
                }
                b'(' => {
                    depth = depth.saturating_add(1);
                    end += 1;
                    self.pos += 1;
                }
                b')' => {
                    depth = depth.saturating_sub(1);
                    end += 1;
                    self.pos += 1;
                }
                b'\n' => break,
                b'#' if depth == 0 => break,
                b'/' if depth == 0 && self.rest().starts_with("//") => break,
                _ => {
                    end += 1;
                    self.pos += 1;
                }
            }
        }
        let token = self.input.get(start..end).unwrap_or_default().trim();
        let token_start = start
            + (self.input.get(start..end).unwrap_or_default().len()
                - self
                    .input
                    .get(start..end)
                    .unwrap_or_default()
                    .trim_start()
                    .len());
        let token_end = token_start + token.len();
        self.node_for(token, token_start..token_end)
    }

    fn node_for(&mut self, token: &str, span: std::ops::Range<usize>) -> Result<NodeId> {
        if token.is_empty() {
            return self.push(Value::Null, "nil", 0..0, true);
        }
        if token.starts_with('"') {
            let text = unquote(token).ok_or_else(|| {
                parse_error(
                    "the SII string is not terminated or has a bad escape",
                    span.start,
                )
            })?;
            return self.push(Value::Str(text), "string", span, false);
        }
        if token.starts_with('(') {
            return self.push(Value::Str(token.to_owned()), "vector", span, true);
        }
        if let Some(hex) = token.strip_prefix('&') {
            if hex.len() == 8
                && let Ok(bits) = u32::from_str_radix(hex, 16)
            {
                return self.push(
                    Value::Float {
                        v: f64::from(f32::from_bits(bits)),
                        width: FloatWidth::F32,
                    },
                    "float",
                    span,
                    false,
                );
            }
            return self.push(Value::Str(token.to_owned()), "token", span, true);
        }
        match token {
            "true" => return self.push(Value::Bool(true), "bool", span, false),
            "false" => return self.push(Value::Bool(false), "bool", span, false),
            "nil" | "null" => return self.push(Value::Null, "nil", span, true),
            _ => {}
        }
        if is_integer(token)
            && let Ok(value) = token.parse::<i128>()
        {
            return self.push(
                Value::Int {
                    v: value,
                    width: IntWidth::Unbounded,
                },
                "int",
                span,
                false,
            );
        }
        if (token.contains('.') || token.contains('e') || token.contains('E'))
            && let Ok(value) = token.parse::<f64>()
        {
            return self.push(
                Value::Float {
                    v: value,
                    width: FloatWidth::F64,
                },
                "float",
                span,
                false,
            );
        }
        self.push(Value::Str(token.to_owned()), "token", span, false)
    }
}

fn is_integer(token: &str) -> bool {
    let digits = token.strip_prefix(['-', '+']).unwrap_or(token);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// Undoes SII's backslash escapes in a quoted string.
fn unquote(token: &str) -> Option<String> {
    let body = token.strip_prefix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return chars.next().is_none().then_some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            },
            other => out.push(other),
        }
    }
    None
}

/// True when the input starts with the SII magic (an optional UTF-8 BOM is allowed).
pub(super) fn plausible(input: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(input) else {
        return false;
    };
    let mut rest = text.strip_prefix('\u{feff}').unwrap_or(text).trim_start();
    loop {
        if rest.starts_with("//") || rest.starts_with('#') {
            rest = rest
                .split_once('\n')
                .map(|(_, tail)| tail.trim_start())
                .unwrap_or_default();
        } else if rest.starts_with("/*") {
            rest = rest
                .split_once("*/")
                .map(|(_, tail)| tail.trim_start())
                .unwrap_or_default();
        } else {
            break;
        }
    }
    rest.starts_with("SiiNunit")
}
