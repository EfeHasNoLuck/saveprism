//! `format.godot-config`: Godot `ConfigFile` text (`.cfg`, `.ini`, `.save`).
//!
//! Written from the public `ConfigFile` documentation and Godot's Variant text syntax. The text is
//! parsed into a data tree; nothing in the file is evaluated.
//!
//! # Tree
//!
//! The root is an `Object` of sections. Its keys are the section names (the unnamed leading
//! section is `""`), and every value is an `Object` of the keys in that section. Empty sections
//! are not represented; comments and blank lines are kept only as bytes of the original text.
//!
//! | ConfigFile value | Node | Hint |
//! |---|---|---|
//! | `true` / `false` | `Bool` | `GODOT:config:bool` |
//! | integer | `Int` (unbounded width, as text has none) | `GODOT:config:int` |
//! | float (`1e10`, `inf`, `-inf`, `nan`) | `Float` (F64) | `GODOT:config:float` |
//! | `"string"` | `Str` | `GODOT:config:string` |
//! | `null` | `Null` (read-only) | `GODOT:config:null` |
//! | `[...]` | `Array` (read-only) | `GODOT:config:array` |
//! | `{...}` | `Map` (read-only; keys stay nodes) | `GODOT:config:dict` |
//! | `Vector2(...)`, `Color(...)`, ... | `Array` of arguments (read-only) | `GODOT:config:constructor:<name>` |
//! | `&"name"` | `Str` (read-only) | `GODOT:config:constructor:StringName` |
//!
//! Editable: booleans, integers, floats and strings, including values inside arrays and
//! constructors. Containers stay read-only, so an edit never changes the shape of the file.
//!
//! # Exact writing
//!
//! The original text is kept as the codec parameter `original`. Every editable node records the
//! byte span of its token; writing replaces only the tokens an edit changed, in Godot's own
//! number and string syntax, and copies every comment, blank line and spacing byte unchanged.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    ByteSpan, Codec, Confidence, DecodeCtx, Decoded, Error, FloatWidth, IntWidth, Node, NodeFlags,
    NodeId, ParamValue, Params, Registry, Result, SaveDoc, SniffCtx, TypeHint, Value, codes,
};

use super::{GODOT_PARSE, GODOT_WRITE, PREFIX};

/// Registers the `format.godot-config` codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GodotConfigCodec));
}

/// The `format.godot-config` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GodotConfigCodec;

impl Codec for GodotConfigCodec {
    fn id(&self) -> &'static str {
        "format.godot-config"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".cfg", ".ini", ".save"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        let has_section = text.lines().any(|line| {
            let line = line.trim();
            line.starts_with('[')
                && line.find(']').is_some_and(|close| {
                    line.get(close + 1..).is_some_and(|tail| {
                        let tail = tail.trim();
                        tail.is_empty() || tail.starts_with(';')
                    })
                })
        });
        let has_assignment = text.lines().any(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('[') {
                return false;
            }
            line.split_once('=').is_some_and(|(key, _)| {
                let key = key.trim();
                !key.is_empty() && !key.contains([':', '"', '{', '}'])
            })
        });
        if has_section && has_assignment {
            if named {
                Confidence::new(0.9)
            } else {
                Confidence::new(0.8)
            }
        } else if named && has_assignment {
            Confidence::new(0.55)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let text = std::str::from_utf8(input)
            .map_err(|_| Error::new(GODOT_PARSE, "Godot ConfigFile is not UTF-8"))?;
        let mut doc = ctx.new_doc();
        let mut sections: Vec<(String, NodeId)> = Vec::new();
        let mut section = String::new();
        let mut keys: Vec<(String, NodeId)> = Vec::new();
        let mut offset = 0usize;
        let mut lines = text.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            let mut consumed = line.len();
            let clean = line.trim();
            if clean.is_empty() || clean.starts_with(';') {
                offset += consumed;
                continue;
            }
            if let Some(rest) = clean.strip_prefix('[')
                && let Some(close) = rest.find(']')
            {
                let tail = rest.get(close + 1..).unwrap_or_default().trim();
                if tail.is_empty() || tail.starts_with(';') {
                    if !keys.is_empty() {
                        let mut node = Node::new(Value::Object(std::mem::take(&mut keys)));
                        node.type_hint = Some(TypeHint::new(format!("{PREFIX}config:section")));
                        node.flags.insert(NodeFlags::READ_ONLY);
                        let id = doc.push(node)?;
                        sections.push((section.clone(), id));
                    }
                    section = rest.get(..close).unwrap_or_default().trim().to_owned();
                    offset += consumed;
                    continue;
                }
            }
            let Some(eq) = line.find('=') else {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("expected section or key=value at byte {offset}"),
                ));
            };
            let key = line.get(..eq).unwrap_or_default().trim().to_owned();
            if key.is_empty() {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("empty key at byte {offset}"),
                ));
            }
            let mut value = line.get(eq + 1..).unwrap_or_default().to_owned();
            let mut value_offset = offset + eq + 1;
            while !balanced(&value) {
                let Some(next) = lines.next() else {
                    return Err(Error::new(
                        GODOT_PARSE,
                        "unterminated multi-line Godot value",
                    ));
                };
                consumed += next.len();
                value.push_str(next);
            }
            let leading = value.len() - value.trim_start().len();
            value_offset += leading;
            let token = value.trim();
            let mut parser = ValueParser {
                src: token,
                base: value_offset,
                at: 0,
                doc: &mut doc,
                depth: 0,
                max_depth: ctx.limits().max_depth,
                force_float: false,
            };
            let id = parser.parse()?;
            parser.ws();
            if parser.at < token.len() {
                let rest = token.get(parser.at..).unwrap_or_default().trim_start();
                if !rest.starts_with(';') {
                    return Err(Error::new(
                        GODOT_PARSE,
                        format!(
                            "unexpected text in value at byte {}",
                            value_offset + parser.at
                        ),
                    ));
                }
            }
            keys.push((key, id));
            offset += consumed;
        }
        if !keys.is_empty() {
            let mut node = Node::new(Value::Object(keys));
            node.type_hint = Some(TypeHint::new(format!("{PREFIX}config:section")));
            node.flags.insert(NodeFlags::READ_ONLY);
            let id = doc.push(node)?;
            sections.push((section, id));
        }
        let mut root = Node::new(Value::Object(sections));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}config:root")));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(
                GODOT_WRITE,
                "original ConfigFile text is missing",
            ));
        };
        let mut patches = Vec::new();
        let mut stack = vec![doc.root()];
        let mut visited = 0usize;
        while let Some(id) = stack.pop() {
            visited = visited.saturating_add(1);
            if visited > doc.len().saturating_add(1) {
                return Err(Error::new(GODOT_WRITE, "cyclic ConfigFile tree"));
            }
            let Some(node) = doc.get(id) else {
                continue;
            };
            stack.extend(doc.children(id));
            if let Value::Map(entries) = &node.value {
                stack.extend(entries.iter().map(|(key, _)| *key));
            }
            if node.lexeme.is_some() || node.flags.contains(NodeFlags::READ_ONLY) {
                continue;
            }
            let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
                continue;
            };
            let Some(span) = node.span else {
                continue;
            };
            let rendered = match (hint, &node.value) {
                ("GODOT:config:bool", Value::Bool(value)) => Some(value.to_string()),
                ("GODOT:config:int", Value::Int { v, .. }) => Some(v.to_string()),
                ("GODOT:config:float", Value::Float { v, .. }) => Some(format_float(*v)),
                ("GODOT:config:string", Value::Str(text)) => Some(escape_string(text)),
                _ => None,
            };
            let Some(rendered) = rendered else {
                continue;
            };
            let start = usize::try_from(span.start)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid ConfigFile token span"))?;
            let end = usize::try_from(span.end)
                .map_err(|_| Error::new(GODOT_WRITE, "invalid ConfigFile token span"))?;
            let old = original.get(start..end).ok_or_else(|| {
                Error::new(
                    GODOT_WRITE,
                    "ConfigFile token span outside the original text",
                )
            })?;
            if old != rendered.as_bytes() {
                patches.push((start, end, rendered.into_bytes()));
            }
        }
        patches.sort_by_key(|(start, _, _)| *start);
        if patches
            .windows(2)
            .any(|pair| match (pair.first(), pair.get(1)) {
                (Some(first), Some(second)) => first.1 > second.0,
                _ => false,
            })
        {
            return Err(Error::new(
                GODOT_WRITE,
                "overlapping ConfigFile token spans",
            ));
        }
        let mut out = original.clone();
        for (start, end, replacement) in patches.into_iter().rev() {
            out.splice(start..end, replacement);
        }
        Ok(out)
    }
}

/// Formats a float so that reading it back gives a float node, not an integer node.
fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value == f64::INFINITY {
        return "inf".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".to_owned();
    }
    let text = value.to_string();
    if text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        text
    } else {
        format!("{text}.0")
    }
}

/// Quotes and escapes a string in Godot's Variant text syntax.
fn escape_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{07}' => out.push_str("\\a"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0b}' => out.push_str("\\v"),
            '\u{0c}' => out.push_str("\\f"),
            character if character < ' ' => {
                out.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// True when every bracket and quote in `value` is closed (comments are ignored).
fn balanced(value: &str) -> bool {
    let mut depth = 0i32;
    let mut quote = false;
    let mut escaped = false;
    let mut comment = false;
    for character in value.chars() {
        if comment {
            if character == '\n' {
                comment = false;
            }
            continue;
        }
        if quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quote = false;
            }
            continue;
        }
        match character {
            '"' => quote = true,
            ';' => comment = true,
            '[' | '{' | '(' => depth += 1,
            ']' | '}' | ')' => depth -= 1,
            _ => {}
        }
    }
    !quote && depth <= 0
}

struct ValueParser<'a, 'd> {
    src: &'a str,
    base: usize,
    at: usize,
    doc: &'d mut SaveDoc,
    depth: usize,
    max_depth: usize,
    force_float: bool,
}

impl ValueParser<'_, '_> {
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
            if self.peek() != Some(b';') {
                break;
            }
            while self
                .src
                .as_bytes()
                .get(self.at)
                .is_some_and(|byte| *byte != b'\n')
            {
                self.at += 1;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.at).copied()
    }

    fn error(&self, message: &str) -> Error {
        Error::new(
            GODOT_PARSE,
            format!("{message} at byte {}", self.base + self.at),
        )
    }

    fn push(
        &mut self,
        value: Value,
        hint: &str,
        start: usize,
        end: usize,
        editable: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(format!("{PREFIX}config:{hint}")));
        node.span = Some(ByteSpan {
            start: len_u64(self.base.saturating_add(start)),
            end: len_u64(self.base.saturating_add(end)),
        });
        if editable {
            node.lexeme = Some("godot:original".into());
        } else {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn parse(&mut self) -> Result<NodeId> {
        self.ws();
        let start = self.at;
        let force_float = self.force_float;
        self.force_float = false;
        let Some(byte) = self.peek() else {
            return Err(self.error("missing ConfigFile value"));
        };
        match byte {
            b'"' => {
                let text = self.parse_string()?;
                self.push(Value::Str(text), "string", start, self.at, true)
            }
            b'[' => self.array(start),
            b'{' => self.dictionary(start),
            b'&' => self.string_name(start),
            _ if self.constructor_end().is_some() => self.constructor(start),
            _ => self.scalar(start, force_float),
        }
    }

    fn parse_string(&mut self) -> Result<String> {
        if self.peek() != Some(b'"') {
            return Err(self.error("expected a quoted Godot string"));
        }
        self.at += 1;
        let mut chunk = self.at;
        let mut out = String::new();
        while let Some(byte) = self.peek() {
            match byte {
                b'"' => {
                    if let Some(text) = self.src.get(chunk..self.at) {
                        out.push_str(text);
                    }
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    if let Some(text) = self.src.get(chunk..self.at) {
                        out.push_str(text);
                    }
                    self.at += 1;
                    let escape = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated Godot string escape"))?;
                    self.at += 1;
                    match escape {
                        b'a' => out.push('\u{07}'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'v' => out.push('\u{0b}'),
                        b'0' => out.push('\0'),
                        b'"' => out.push('"'),
                        b'\'' => out.push('\''),
                        b'\\' => out.push('\\'),
                        b'\n' => {}
                        b'u' => out.push(self.unicode_escape(4)?),
                        b'U' => out.push(self.unicode_escape(6)?),
                        _ => return Err(self.error("unsupported Godot string escape")),
                    }
                    chunk = self.at;
                }
                _ => self.at += 1,
            }
        }
        Err(self.error("unterminated Godot string"))
    }

    fn unicode_escape(&mut self, digits: usize) -> Result<char> {
        let end = self
            .at
            .checked_add(digits)
            .filter(|end| *end <= self.src.len())
            .ok_or_else(|| self.error("truncated Godot unicode escape"))?;
        let text = self
            .src
            .get(self.at..end)
            .ok_or_else(|| self.error("invalid Godot unicode escape"))?;
        let code = u32::from_str_radix(text, 16)
            .map_err(|_| self.error("invalid Godot unicode escape"))?;
        let character =
            char::from_u32(code).ok_or_else(|| self.error("invalid Godot unicode code point"))?;
        self.at = end;
        Ok(character)
    }

    fn constructor_end(&self) -> Option<usize> {
        let bytes = self.src.as_bytes();
        let mut at = self.at;
        let first = bytes.get(at).copied()?;
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return None;
        }
        while bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            at += 1;
        }
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        (bytes.get(at) == Some(&b'(')).then_some(at)
    }

    fn scalar(&mut self, start: usize, force_float: bool) -> Result<NodeId> {
        while self.peek().is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b',' | b']' | b'}' | b')' | b';' | b':')
        }) {
            self.at += 1;
        }
        let token = self.src.get(start..self.at).unwrap_or_default();
        if token == "true" || token == "false" {
            return self.push(Value::Bool(token == "true"), "bool", start, self.at, true);
        }
        if token == "null" {
            return self.push(Value::Null, "null", start, self.at, false);
        }
        if let Some(value) = parse_number(token) {
            return match value {
                Value::Int { v, .. } if force_float => self.push(
                    Value::Float {
                        v: v as f64,
                        width: FloatWidth::F64,
                    },
                    "float",
                    start,
                    self.at,
                    true,
                ),
                Value::Int { .. } => self.push(value, "int", start, self.at, true),
                Value::Float { .. } => self.push(value, "float", start, self.at, true),
                _ => Err(self.error("unsupported Godot number")),
            };
        }
        Err(self.error("unsupported Godot value"))
    }

    fn array(&mut self, start: usize) -> Result<NodeId> {
        self.enter()?;
        self.at += 1;
        self.ws();
        let mut items = Vec::new();
        loop {
            if self.peek() == Some(b']') {
                self.at += 1;
                break;
            }
            if self.at >= self.src.len() {
                return Err(self.error("unterminated Godot array"));
            }
            items.push(self.parse()?);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.ws();
                }
                Some(b']') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected comma in Godot array")),
            }
        }
        self.depth -= 1;
        self.push(Value::Array(items), "array", start, self.at, false)
    }

    fn dictionary(&mut self, start: usize) -> Result<NodeId> {
        self.enter()?;
        self.at += 1;
        self.ws();
        let mut entries = Vec::new();
        loop {
            if self.peek() == Some(b'}') {
                self.at += 1;
                break;
            }
            if self.at >= self.src.len() {
                return Err(self.error("unterminated Godot dictionary"));
            }
            let key = self.parse()?;
            self.ws();
            if self.peek() != Some(b':') {
                return Err(self.error("dictionary entry lacks a colon"));
            }
            self.at += 1;
            self.ws();
            let value = self.parse()?;
            entries.push((key, value));
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.ws();
                }
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected comma in Godot dictionary")),
            }
        }
        self.depth -= 1;
        self.push(Value::Map(entries), "dict", start, self.at, false)
    }

    fn constructor(&mut self, start: usize) -> Result<NodeId> {
        self.enter()?;
        let name_start = self.at;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.at += 1;
        }
        let name = self
            .src
            .get(name_start..self.at)
            .unwrap_or_default()
            .to_owned();
        self.ws();
        if self.peek() != Some(b'(') {
            return Err(self.error("expected '(' after a Godot constructor name"));
        }
        self.at += 1;
        self.ws();
        let mut items = Vec::new();
        let float_arguments = float_constructor(&name);
        loop {
            if self.peek() == Some(b')') {
                self.at += 1;
                break;
            }
            if self.at >= self.src.len() {
                return Err(self.error("unterminated Godot constructor"));
            }
            self.force_float = float_arguments;
            let item = self.parse()?;
            self.force_float = false;
            items.push(item);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                    self.ws();
                }
                Some(b')') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected comma in Godot constructor")),
            }
        }
        self.depth -= 1;
        self.push(
            Value::Array(items),
            &format!("constructor:{name}"),
            start,
            self.at,
            false,
        )
    }

    fn string_name(&mut self, start: usize) -> Result<NodeId> {
        self.at += 1;
        self.ws();
        let text = self.parse_string()?;
        self.push(
            Value::Str(text),
            "constructor:StringName",
            start,
            self.at,
            false,
        )
    }

    fn enter(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!(
                    "ConfigFile nesting limit of {} exceeded at byte {}",
                    self.max_depth,
                    self.base + self.at
                ),
            ));
        }
        self.depth += 1;
        Ok(())
    }
}

/// True when the constructor's arguments are floating-point numbers even if written as integers.
fn float_constructor(name: &str) -> bool {
    matches!(
        name,
        "Vector2"
            | "Vector3"
            | "Vector4"
            | "Rect2"
            | "Plane"
            | "Quaternion"
            | "AABB"
            | "Basis"
            | "Transform2D"
            | "Transform3D"
            | "Projection"
            | "Color"
            | "PackedFloat32Array"
            | "PackedFloat64Array"
    )
}

/// Parses a Godot numeric token; `None` when the token is not a number.
fn parse_number(token: &str) -> Option<Value> {
    let clean: String = token
        .chars()
        .filter(|character| *character != '_')
        .collect();
    let (sign, unsigned) = match clean.strip_prefix('-') {
        Some(rest) => (-1i128, rest),
        None => (1i128, clean.strip_prefix('+').unwrap_or(&clean)),
    };
    match unsigned {
        "inf" => {
            return Some(Value::Float {
                v: if sign < 0 {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                width: FloatWidth::F64,
            });
        }
        "nan" => {
            return Some(Value::Float {
                v: f64::NAN,
                width: FloatWidth::F64,
            });
        }
        _ => {}
    }
    if let Some(hex) = unsigned.strip_prefix("0x") {
        let value = i128::from_str_radix(hex, 16).ok()?.checked_mul(sign)?;
        return Some(Value::Int {
            v: value,
            width: IntWidth::Unbounded,
        });
    }
    let float_like = unsigned
        .bytes()
        .any(|byte| matches!(byte, b'.' | b'e' | b'E'));
    if !float_like && let Ok(value) = clean.parse::<i128>() {
        return Some(Value::Int {
            v: value,
            width: IntWidth::Unbounded,
        });
    }
    clean.parse::<f64>().ok().map(|value| Value::Float {
        v: value,
        width: FloatWidth::F64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use saveprism_core::{Edit, Limits, OwnedValue, Path};

    fn decode(input: &[u8]) -> Decoded {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        GodotConfigCodec
            .decode(input, &mut ctx)
            .expect("ConfigFile parses")
    }

    fn encode(decoded: &Decoded) -> Vec<u8> {
        GodotConfigCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("ConfigFile writes")
    }

    fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
        saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value,
            }],
            &Limits::default(),
        )
        .expect("edit applies");
    }

    #[test]
    fn sniff_requires_config_like_structure() {
        let codec = GodotConfigCodec;
        let json = SniffCtx {
            file_name: Some("array.save"),
            depth: 0,
        };
        assert_eq!(codec.sniff(b"[1, 2, 3]", &json), Confidence::NONE);
        let config = SniffCtx {
            file_name: Some("settings.cfg"),
            depth: 0,
        };
        assert!(codec.sniff(b"[Player]\nlevel=4\n", &config) > Confidence::NONE);
    }

    #[test]
    fn preserves_config_text_and_patches_only_edited_value() {
        let input = b"; leading comment\n[ Player ]\n  gold   =  42 ; note\nname=\"Mira\\nRune\"\nitems=[1, 2, Vector2(3, 4)]\noptions={\"active\": true}\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/Player/gold", OwnedValue::Int(7));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("  gold   =  7 ; note"));
        assert!(text.contains("; leading comment"));
        assert!(text.contains("Vector2(3, 4)"));
    }

    #[test]
    fn edits_every_scalar_kind_and_keeps_the_rest() {
        let input = b"[Game]\nint = -12\nfloat = 1e10\ntext = \"old\"\nflag = false\n";
        let mut decoded = decode(input);
        set(&mut decoded, "/Game/int", OwnedValue::Int(99));
        set(&mut decoded, "/Game/float", OwnedValue::Float(2.5));
        set(
            &mut decoded,
            "/Game/text",
            OwnedValue::Str("new\nline".into()),
        );
        set(&mut decoded, "/Game/flag", OwnedValue::Bool(true));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert_eq!(
            text,
            "[Game]\nint = 99\nfloat = 2.5\ntext = \"new\\nline\"\nflag = true\n"
        );
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn floats_stay_floats_when_edited() {
        let input = b"[A]\nvalue = 1.25\n";
        let mut decoded = decode(input);
        set(&mut decoded, "/A/value", OwnedValue::Float(2.0));
        let output = encode(&decoded);
        assert_eq!(output, b"[A]\nvalue = 2.0\n");
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn dictionary_keys_stay_nodes_and_constructors_edit_inside() {
        let input = b"[A]\nmap = {\"k\": 1, 2: 3, \"nested\": {}}\npos = Vector2(1, 2)\nname = &\"hero\"\npath = NodePath(\"World/Player\")\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/A/map/0", OwnedValue::Int(9));
        set(&mut decoded, "/A/map/1.key", OwnedValue::Int(7));
        set(&mut decoded, "/A/pos/0", OwnedValue::Float(4.5));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("{\"k\": 9, 7: 3, \"nested\": {}}"));
        assert!(text.contains("Vector2(4.5, 2)"));
        assert!(text.contains("&\"hero\""));
        assert!(text.contains("NodePath(\"World/Player\")"));
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn multi_line_values_and_special_numbers_round_trip() {
        let input = b"; top\n[Odd]\nvalues = [\n  1,\n  2.5, ; inside\n  \"three\"\n]\nmap = {\n  \"a\": 1,\n  \"b\": inf,\n}\nneg = -inf\nnan = nan\nbig = 1e10\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/Odd/values/1", OwnedValue::Float(9.5));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("  9.5, ; inside"));
        assert!(text.contains("; top"));
    }

    #[test]
    fn godot_string_escapes_are_understood() {
        let input = b"[S]\nvalue = \"a\\tb\\u00e9\\U01f600\\\\\\\"\"\n";
        let decoded = decode(input);
        let id = decoded
            .doc
            .resolve(&Path::parse("/S/value").expect("path"))
            .expect("value");
        assert_eq!(
            decoded.doc.get(id).map(|node| &node.value),
            Some(&Value::Str("a\tbé😀\\\"".into()))
        );
        assert_eq!(encode(&decoded), input);
    }

    #[test]
    fn generated_config_fixture_is_byte_exact() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/godot/config.cfg");
        let input = std::fs::read(path).expect("fixture exists");
        let decoded = decode(&input);
        assert_eq!(encode(&decoded), input);
    }

    use proptest::prelude::*;

    fn writes_back(bytes: &[u8]) {
        if let Ok(decoded) = GodotConfigCodec.decode(bytes, &mut DecodeCtx::new(&Limits::default()))
        {
            let written = GodotConfigCodec
                .encode(&decoded.doc, &decoded.params)
                .expect("decoded ConfigFile writes");
            let again = GodotConfigCodec
                .decode(&written, &mut DecodeCtx::new(&Limits::default()))
                .expect("written ConfigFile decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
            writes_back(&body);
        }

        #[test]
        fn config_like_input_never_panics(
            body in proptest::collection::vec(
                prop_oneof![
                    proptest::sample::select(b"[A]\nk=v;\n\"\\[]{}(),:&".to_vec()),
                    0_u8..=127,
                    any::<u8>(),
                ],
                0..128,
            )
        ) {
            writes_back(&body);
        }
    }
}
