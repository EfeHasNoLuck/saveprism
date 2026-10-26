//! Shared Godot Variant text parser and byte-exact span writer.
//!
//! Used by the `ConfigFile` codec (`config.rs`) and the text-resource codec (`resource_text.rs`),
//! so both understand the same number, string, array, dictionary and constructor syntax. The
//! original text is kept as bytes; every editable token records its span and writing replaces only
//! the tokens an edit changed.
//!
//! The syntax follows the public Godot Variant text format. Constructors are stored as read-only
//! argument arrays; the caller can list names (`ExtResource`, `SubResource`) whose whole subtree
//! must stay read-only, and can allow typed containers (`Array[int]([...])`), which also stay
//! read-only.

use saveprism_core::{
    ByteSpan, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint,
    Value, limits::len_u64,
};

use super::{GODOT_PARSE, GODOT_WRITE};

/// How one file's Variant text is parsed and written.
#[derive(Clone, Copy)]
pub(crate) struct TextOptions {
    /// Hint prefix on every parsed node (`GODOT:config:` or `GODOT:tres:`).
    pub(crate) prefix: &'static str,
    /// Constructor names whose arguments stay read-only (references, script classes).
    pub(crate) locked_constructors: &'static [&'static str],
    /// Whether typed containers such as `Array[int](...)` are accepted read-only.
    pub(crate) typed_containers: bool,
}

/// Parses one Variant text token, records its spans in `doc` and returns the node.
pub(crate) fn parse_value(
    token: &str,
    base: usize,
    doc: &mut SaveDoc,
    max_depth: usize,
    options: TextOptions,
) -> Result<NodeId> {
    let mut parser = ValueParser {
        src: token,
        base,
        at: 0,
        doc,
        depth: 0,
        max_depth,
        force_float: false,
        locked: false,
        options,
    };
    let id = parser.parse()?;
    parser.ws();
    if parser.at < token.len() {
        let rest = token.get(parser.at..).unwrap_or_default().trim_start();
        if !rest.starts_with(';') {
            return Err(Error::new(
                GODOT_PARSE,
                format!("unexpected text in value at byte {}", base + parser.at),
            ));
        }
    }
    Ok(id)
}

/// Writes `doc` back into `original`, replacing only the tokens an edit changed.
pub(crate) fn write_patched(original: &[u8], doc: &SaveDoc, prefix: &str) -> Result<Vec<u8>> {
    let mut patches = Vec::new();
    let mut stack = vec![doc.root()];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(GODOT_WRITE, "cyclic Godot text tree"));
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
        let Some(suffix) = hint.strip_prefix(prefix) else {
            continue;
        };
        let Some(span) = node.span else {
            continue;
        };
        let rendered = match (suffix, &node.value) {
            ("bool", Value::Bool(value)) => Some(value.to_string()),
            ("int", Value::Int { v, .. }) => Some(v.to_string()),
            ("float", Value::Float { v, .. }) => Some(format_float(*v)),
            ("string", Value::Str(text)) => Some(escape_string(text)),
            _ => None,
        };
        let Some(rendered) = rendered else {
            continue;
        };
        let start = usize::try_from(span.start)
            .map_err(|_| Error::new(GODOT_WRITE, "invalid Godot text token span"))?;
        let end = usize::try_from(span.end)
            .map_err(|_| Error::new(GODOT_WRITE, "invalid Godot text token span"))?;
        let old = original
            .get(start..end)
            .ok_or_else(|| Error::new(GODOT_WRITE, "Godot text token span outside the input"))?;
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
            "overlapping Godot text token spans",
        ));
    }
    let mut out = original.to_vec();
    for (start, end, replacement) in patches.into_iter().rev() {
        out.splice(start..end, replacement);
    }
    Ok(out)
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
pub(crate) fn balanced(value: &str) -> bool {
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
    locked: bool,
    options: TextOptions,
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
        node.type_hint = Some(TypeHint::new(format!("{}{hint}", self.options.prefix)));
        node.span = Some(ByteSpan {
            start: len_u64(self.base.saturating_add(start)),
            end: len_u64(self.base.saturating_add(end)),
        });
        if editable && !self.locked {
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
            return Err(self.error("missing Godot value"));
        };
        match byte {
            b'"' => {
                let text = self.parse_string()?;
                self.push(Value::Str(text), "string", start, self.at, true)
            }
            b'[' => self.array(start),
            b'{' => self.dictionary(start),
            b'&' => self.string_name(start),
            _ if self.options.typed_containers && self.typed_container_start() => {
                self.typed_container(start)
            }
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

    fn typed_container_start(&self) -> bool {
        let bytes = self.src.as_bytes();
        let mut at = self.at;
        let Some(first) = bytes.get(at).copied() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == b'_') {
            return false;
        }
        while bytes
            .get(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            at += 1;
        }
        let name = self.src.get(self.at..at).unwrap_or_default();
        if name != "Array" && name != "Dictionary" {
            return false;
        }
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        bytes.get(at) == Some(&b'[')
    }

    fn typed_container(&mut self, _start: usize) -> Result<NodeId> {
        self.enter()?;
        while self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.at += 1;
        }
        self.skip_type_spec()?;
        self.ws();
        if self.peek() != Some(b'(') {
            return Err(self.error("expected '(' after a Godot typed container"));
        }
        self.at += 1;
        self.ws();
        let previous = self.locked;
        self.locked = true;
        let inner = self.parse()?;
        self.locked = previous;
        self.ws();
        if self.peek() != Some(b')') {
            return Err(self.error("unterminated Godot typed container"));
        }
        self.at += 1;
        self.depth -= 1;
        Ok(inner)
    }

    fn skip_type_spec(&mut self) -> Result<()> {
        if self.peek() != Some(b'[') {
            return Err(self.error("expected '[' in a Godot typed container"));
        }
        let mut depth = 0usize;
        while let Some(byte) = self.peek() {
            match byte {
                b'[' => {
                    depth += 1;
                    self.at += 1;
                }
                b']' => {
                    depth = depth.saturating_sub(1);
                    self.at += 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                b'"' => {
                    self.parse_string()?;
                }
                _ => self.at += 1,
            }
        }
        Err(self.error("unterminated Godot typed-container descriptor"))
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
        let locked_constructor = self
            .options
            .locked_constructors
            .iter()
            .any(|locked| *locked == name);
        let previous_locked = self.locked;
        if locked_constructor {
            self.locked = true;
        }
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
        self.locked = previous_locked;
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
                saveprism_core::codes::CORE_LIMIT_DEPTH,
                format!(
                    "Godot text nesting limit of {} exceeded at byte {}",
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
