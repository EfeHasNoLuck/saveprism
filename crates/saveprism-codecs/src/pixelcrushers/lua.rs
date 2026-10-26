//! A data-only reader and span writer for the Lua assignment text the Dialogue System saves.
//!
//! The Dialogue System's `PersistentDataManager.GetSaveData()` returns "human-readable Lua code"
//! (public class reference) of the form `Variable["Gold"]=10; Actor[1].Name="Mira";`. This module
//! reads only literals: numbers, strings, booleans, `nil`, tables and plain assignment paths.
//! Nothing is evaluated, no function is called and no identifier is resolved.
//!
//! # Tree
//!
//! The root is a read-only `Object` (`LUA:assignments`) whose keys are the assignment targets in
//! source order and whose values are the literals. A table is a read-only `Object`
//! (`LUA:table`); keys keep their source spelling (`gold`, `["Gold"]`, `[1]`). Numbers, strings
//! and booleans are editable and keep the byte span of their token, so writing patches only the
//! tokens an edit changed; `nil`, tables and the root stay read-only.

use saveprism_core::limits::len_u64;
use saveprism_core::{
    DecodeCtx, Error, ErrorCode, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc,
    TypeHint, Value,
};

use crate::text_common::{append_numbered, apply_patches, format_float, span_range};

/// The Lua assignment text is not the data-only subset this reader accepts.
pub const LUA_PARSE: ErrorCode = ErrorCode::new("SP-LUA-001");
/// A changed Lua literal cannot be written.
pub const LUA_WRITE: ErrorCode = ErrorCode::new("SP-LUA-002");

const ASSIGNMENTS_HINT: &str = "LUA:assignments";
const TABLE_HINT: &str = "LUA:table";
const INT_HINT: &str = "LUA:int";
const FLOAT_HINT: &str = "LUA:float";
const BOOL_HINT: &str = "LUA:bool";
const STRING_HINT: &str = "LUA:string";
const NIL_HINT: &str = "LUA:nil";

/// True when `text` starts like a Dialogue System Lua save (a known table assignment).
pub(crate) fn looks_like_lua(text: &str) -> bool {
    let trimmed = text.trim_start();
    [
        "Variable[",
        "Actor[",
        "Item[",
        "Location[",
        "Conversation[",
        "Quest[",
        "Relationship[",
        "Status[",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

fn bad(message: impl Into<String>, offset: usize) -> Error {
    Error::new(LUA_PARSE, message).with_offset(len_u64(offset))
}

/// Parses the data-only subset into a document.
pub(crate) fn parse(text: &str, ctx: &DecodeCtx<'_>) -> Result<SaveDoc> {
    let mut parser = Parser {
        input: text,
        bytes: text.as_bytes(),
        pos: 0,
        depth: 0,
        max_depth: ctx.limits().max_depth,
        doc: ctx.new_doc(),
    };
    parser.parse_assignments()?;
    ctx.limits().check_input(len_u64(text.len()))?;
    Ok(parser.doc)
}

/// Writes one nested Lua record back; returns the original text when nothing changed.
pub(crate) fn write_nested(original: &str, doc: &SaveDoc, root: NodeId) -> Result<Vec<u8>> {
    let mut patches: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    let mut stack = vec![root];
    let mut visited = 0usize;
    while let Some(id) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > doc.len().saturating_add(1) {
            return Err(Error::new(LUA_WRITE, "cyclic Lua data is not writable"));
        }
        let Some(node) = doc.get(id) else {
            continue;
        };
        stack.extend(doc.children(id));
        if node.lexeme.is_some() || node.flags.contains(NodeFlags::READ_ONLY) {
            continue;
        }
        let Some(hint) = node.type_hint.as_ref().map(TypeHint::as_str) else {
            continue;
        };
        let Some(replacement) = render(hint, &node.value)? else {
            continue;
        };
        let (start, end) = span_range(node, LUA_WRITE)?;
        let old = original.as_bytes().get(start..end).ok_or_else(|| {
            Error::new(LUA_WRITE, "a Lua token span is outside the original text")
        })?;
        if old != replacement.as_slice() {
            patches.push((start, end, replacement));
        }
    }
    apply_patches(original.as_bytes(), LUA_WRITE, &mut patches)
}

fn render(hint: &str, value: &Value) -> Result<Option<Vec<u8>>> {
    let text = match (hint, value) {
        (INT_HINT, Value::Int { v, .. }) => v.to_string(),
        (FLOAT_HINT, Value::Float { v, .. }) => format_float(*v),
        (BOOL_HINT, Value::Bool(value)) => value.to_string(),
        (STRING_HINT, Value::Str(text)) => quote(text),
        _ => return Ok(None),
    };
    Ok(Some(text.into_bytes()))
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if character.is_control() => {
                out.push_str(&format!("\\{:03}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

struct Parser<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
    max_depth: usize,
    doc: SaveDoc,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn fail(&self, message: &str) -> Error {
        bad(message, self.pos)
    }

    fn skip_ws(&mut self) {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.pos = self.pos.saturating_add(1);
            }
            if self.peek() == Some(b'-')
                && self.bytes.get(self.pos.saturating_add(1)) == Some(&b'-')
            {
                while let Some(byte) = self.peek() {
                    self.pos = self.pos.saturating_add(1);
                    if byte == b'\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }

    fn parse_assignments(&mut self) -> Result<NodeId> {
        let mut entries = Vec::new();
        loop {
            self.skip_ws();
            if self.peek().is_none() {
                break;
            }
            let target_start = self.pos;
            let Some(eq) = self.find_assignment_eq() else {
                return Err(self.fail("expected an assignment"));
            };
            let target = self
                .input
                .get(target_start..eq)
                .ok_or_else(|| self.fail("invalid assignment target"))?
                .trim()
                .to_owned();
            if !valid_target(&target) {
                return Err(bad(
                    format!("unsupported Lua statement target `{target}`"),
                    target_start,
                ));
            }
            self.pos = eq.saturating_add(1);
            self.skip_ws();
            let value = self.value()?;
            entries.push((target, value));
            self.skip_ws();
            if self.peek() == Some(b';') {
                self.pos = self.pos.saturating_add(1);
            }
        }
        let mut node = Node::new(Value::Object(entries));
        node.type_hint = Some(TypeHint::new(ASSIGNMENTS_HINT));
        node.flags.insert(NodeFlags::READ_ONLY);
        let id = self.doc.push(node)?;
        self.doc.set_root(id);
        Ok(id)
    }

    fn find_assignment_eq(&self) -> Option<usize> {
        let mut index = self.pos;
        let mut depth = 0usize;
        let mut quote = None;
        while let Some(&byte) = self.bytes.get(index) {
            if let Some(open) = quote {
                if byte == b'\\' {
                    index = index.saturating_add(2);
                    continue;
                }
                if byte == open {
                    quote = None;
                }
                index = index.saturating_add(1);
                continue;
            }
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'[' | b'(' | b'{' => depth = depth.saturating_add(1),
                b']' | b')' | b'}' => depth = depth.saturating_sub(1),
                b'=' if depth == 0 => return Some(index),
                b';' | b'\n' if depth == 0 => return None,
                _ => {}
            }
            index = index.saturating_add(1);
        }
        None
    }

    fn enter(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(self.fail("Lua table nesting limit exceeded"));
        }
        self.depth = self.depth.saturating_add(1);
        Ok(())
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn value(&mut self) -> Result<NodeId> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'"' | b'\'') => self.string(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b't') if self.starts_with("true") => self.boolean(true),
            Some(b'f') if self.starts_with("false") => self.boolean(false),
            Some(b'n') if self.starts_with("nil") => self.nil(),
            Some(_) => Err(self.fail("unsupported Lua expression (data literals only)")),
            None => Err(self.fail("unexpected end of Lua text")),
        }
    }

    fn starts_with(&self, text: &str) -> bool {
        self.input
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(text))
    }

    fn boolean(&mut self, value: bool) -> Result<NodeId> {
        let text = if value { "true" } else { "false" };
        let start = self.pos;
        self.pos = self.pos.saturating_add(text.len());
        self.scalar(Value::Bool(value), BOOL_HINT, start, self.pos)
    }

    fn nil(&mut self) -> Result<NodeId> {
        let start = self.pos;
        self.pos = self.pos.saturating_add(3);
        let mut node = Node::new(Value::Null);
        node.type_hint = Some(TypeHint::new(NIL_HINT));
        node.span = Some(span(start, self.pos));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(node)
    }

    fn number(&mut self) -> Result<NodeId> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos = self.pos.saturating_add(1);
        }
        let hex = self.starts_with("0x") || self.starts_with("0X");
        if hex {
            self.pos = self.pos.saturating_add(2);
            let digits = self.take_while(is_hex_digit);
            if digits == 0 {
                return Err(bad("a hexadecimal Lua number has no digits", start));
            }
        } else {
            let mut digits = self.take_while(u8::is_ascii_digit);
            if self.peek() == Some(b'.') {
                self.pos = self.pos.saturating_add(1);
                digits = digits.saturating_add(self.take_while(u8::is_ascii_digit));
            }
            if digits == 0 {
                return Err(bad("a Lua number has no digits", start));
            }
            if self.peek().is_some_and(|byte| matches!(byte, b'e' | b'E')) {
                let exponent = self.pos;
                self.pos = self.pos.saturating_add(1);
                if self.peek().is_some_and(|byte| matches!(byte, b'+' | b'-')) {
                    self.pos = self.pos.saturating_add(1);
                }
                if self.take_while(u8::is_ascii_digit) == 0 {
                    self.pos = exponent;
                    return Err(bad("a Lua exponent has no digits", exponent));
                }
            }
        }
        let token = self
            .input
            .get(start..self.pos)
            .ok_or_else(|| bad("invalid Lua number", start))?;
        if hex || (!token.contains('.') && !token.contains('e') && !token.contains('E')) {
            let (negative, body) = if let Some(rest) = token.strip_prefix('-') {
                (true, rest)
            } else {
                (false, token)
            };
            let value =
                if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                    i128::from_str_radix(hex, 16)
                } else {
                    body.parse::<i128>()
                }
                .map_err(|_| bad(format!("unsupported Lua number `{token}`"), start))?;
            let value = if negative { -value } else { value };
            return self.scalar(
                Value::Int {
                    v: value,
                    width: IntWidth::Unbounded,
                },
                INT_HINT,
                start,
                self.pos,
            );
        }
        let value = token
            .parse::<f64>()
            .map_err(|_| bad(format!("unsupported Lua number `{token}`"), start))?;
        self.scalar(
            Value::Float {
                v: value,
                width: FloatWidth::F64,
            },
            FLOAT_HINT,
            start,
            self.pos,
        )
    }

    fn take_while(&mut self, predicate: fn(&u8) -> bool) -> usize {
        let start = self.pos;
        while self.peek().is_some_and(|byte| predicate(&byte)) {
            self.pos = self.pos.saturating_add(1);
        }
        self.pos.saturating_sub(start)
    }

    fn scalar(&mut self, value: Value, hint: &str, start: usize, end: usize) -> Result<NodeId> {
        let token = self
            .input
            .get(start..end)
            .ok_or_else(|| bad("invalid Lua token", start))?;
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(hint));
        node.span = Some(span(start, end));
        node.lexeme = Some(token.into());
        self.doc.push(node)
    }

    fn string(&mut self) -> Result<NodeId> {
        let start = self.pos;
        let quote_byte = self
            .peek()
            .ok_or_else(|| self.fail("unexpected end of Lua text"))?;
        self.pos = self.pos.saturating_add(1);
        let mut text = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(bad("unterminated Lua string", start));
            };
            if byte == quote_byte {
                self.pos = self.pos.saturating_add(1);
                break;
            }
            if byte == b'\\' {
                self.pos = self.pos.saturating_add(1);
                self.escape(&mut text)?;
                continue;
            }
            let character = self
                .input
                .get(self.pos..)
                .and_then(|rest| rest.chars().next())
                .ok_or_else(|| bad("invalid UTF-8 in a Lua string", self.pos))?;
            text.push(character);
            self.pos = self.pos.saturating_add(character.len_utf8());
        }
        self.scalar(Value::Str(text), STRING_HINT, start, self.pos)
    }

    fn escape(&mut self, text: &mut String) -> Result<()> {
        let Some(byte) = self.peek() else {
            return Err(self.fail("unterminated Lua escape"));
        };
        self.pos = self.pos.saturating_add(1);
        match byte {
            b'a' => text.push('\u{7}'),
            b'b' => text.push('\u{8}'),
            b'f' => text.push('\u{c}'),
            b'n' => text.push('\n'),
            b'r' => text.push('\r'),
            b't' => text.push('\t'),
            b'v' => text.push('\u{b}'),
            b'\\' => text.push('\\'),
            b'"' => text.push('"'),
            b'\'' => text.push('\''),
            b'\n' => text.push('\n'),
            b'z' => {
                while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                    self.pos = self.pos.saturating_add(1);
                }
            }
            b'x' => {
                let digits = self.input.get(self.pos..).unwrap_or_default();
                let (value, used) = take_hex_digits(digits, 2);
                if used == 0 {
                    return Err(self.fail("a Lua \\x escape has no hex digits"));
                }
                self.pos = self.pos.saturating_add(used);
                text.push(char::from(value));
            }
            b'u' => {
                if self.peek() != Some(b'{') {
                    return Err(self.fail("a Lua \\u escape needs braces"));
                }
                self.pos = self.pos.saturating_add(1);
                let digits = self.input.get(self.pos..).unwrap_or_default();
                let (value, used) = take_hex_digits_wide(digits);
                if used == 0 || value > 0x10ffff {
                    return Err(self.fail("invalid Lua \\u escape"));
                }
                self.pos = self.pos.saturating_add(used);
                if self.peek() != Some(b'}') {
                    return Err(self.fail("unterminated Lua \\u escape"));
                }
                self.pos = self.pos.saturating_add(1);
                let character = char::from_u32(value)
                    .ok_or_else(|| self.fail("invalid Unicode scalar in a Lua string"))?;
                text.push(character);
            }
            b'0'..=b'9' => {
                let mut value = u32::from(byte.saturating_sub(b'0'));
                for _ in 0..2 {
                    let Some(next) = self.peek() else {
                        break;
                    };
                    if !next.is_ascii_digit() {
                        break;
                    }
                    self.pos = self.pos.saturating_add(1);
                    value = value
                        .saturating_mul(10)
                        .saturating_add(u32::from(next.saturating_sub(b'0')));
                }
                let character =
                    char::from_u32(value).ok_or_else(|| self.fail("invalid decimal Lua escape"))?;
                text.push(character);
            }
            other => {
                return Err(bad(
                    format!("unsupported Lua escape \\{}", char::from(other)),
                    self.pos.saturating_sub(1),
                ));
            }
        }
        Ok(())
    }

    fn table(&mut self) -> Result<NodeId> {
        let start = self.pos;
        self.enter()?;
        let result = self.table_inner(start);
        self.leave();
        result
    }

    fn table_inner(&mut self, start: usize) -> Result<NodeId> {
        self.pos = self.pos.saturating_add(1);
        let mut entries = Vec::new();
        let mut position = 0usize;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'}') => {
                    self.pos = self.pos.saturating_add(1);
                    break;
                }
                None => return Err(bad("unterminated Lua table", start)),
                _ => {}
            }
            let keyed = self.key_ahead()?;
            if let Some(key) = keyed {
                self.skip_ws();
                if self.peek() != Some(b'=') {
                    return Err(self.fail("expected '=' after a Lua table key"));
                }
                self.pos = self.pos.saturating_add(1);
                self.skip_ws();
                let value = self.value()?;
                append_numbered(&mut self.doc, &mut entries, TABLE_HINT, key, value)?;
            } else {
                position = position.saturating_add(1);
                let value = self.value()?;
                append_numbered(
                    &mut self.doc,
                    &mut entries,
                    TABLE_HINT,
                    format!("[{position}]"),
                    value,
                )?;
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') | Some(b';') => {
                    self.pos = self.pos.saturating_add(1);
                }
                Some(b'}') => {}
                None => return Err(bad("unterminated Lua table", start)),
                _ => return Err(self.fail("expected ',' or '}' in a Lua table")),
            }
        }
        let mut node = Node::new(Value::Object(entries));
        node.type_hint = Some(TypeHint::new(TABLE_HINT));
        node.span = Some(span(start, self.pos));
        node.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(node)
    }

    /// If the next entry is `name = ...` or `[expr] = ...`, returns its key text without consuming
    /// anything on a positional entry.
    fn key_ahead(&mut self) -> Result<Option<String>> {
        let saved = self.pos;
        self.skip_ws();
        let key = if self.peek() == Some(b'[') {
            let key_start = self.pos;
            let mut index = self.pos.saturating_add(1);
            let mut quote = None;
            let mut depth = 1usize;
            while let Some(&byte) = self.bytes.get(index) {
                if let Some(open) = quote {
                    if byte == b'\\' {
                        index = index.saturating_add(2);
                        continue;
                    }
                    if byte == open {
                        quote = None;
                    }
                } else {
                    match byte {
                        b'"' | b'\'' => quote = Some(byte),
                        b'[' => depth = depth.saturating_add(1),
                        b']' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                index = index.saturating_add(1);
            }
            if depth != 0 {
                self.pos = saved;
                return Ok(None);
            }
            self.pos = index.saturating_add(1);
            let text = self
                .input
                .get(key_start..self.pos)
                .unwrap_or_default()
                .to_owned();
            Some(text)
        } else if self
            .peek()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        {
            let name_start = self.pos;
            self.take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
            let text = self
                .input
                .get(name_start..self.pos)
                .unwrap_or_default()
                .to_owned();
            let before_eq = self.pos;
            self.skip_ws();
            if self.peek() == Some(b'=') {
                Some(text)
            } else {
                self.pos = before_eq;
                None
            }
        } else {
            None
        };
        if key.is_some() {
            self.skip_ws();
            Ok(key)
        } else {
            self.pos = saved;
            Ok(None)
        }
    }
}

fn span(start: usize, end: usize) -> saveprism_core::ByteSpan {
    saveprism_core::ByteSpan {
        start: len_u64(start),
        end: len_u64(end),
    }
}

fn is_hex_digit(byte: &u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn take_hex_digits(text: &str, max: usize) -> (u8, usize) {
    let mut value = 0u32;
    let mut used = 0usize;
    for byte in text.as_bytes().iter().take(max) {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte.saturating_sub(b'0')),
            b'a'..=b'f' => u32::from(byte.saturating_sub(b'a')).saturating_add(10),
            b'A'..=b'F' => u32::from(byte.saturating_sub(b'A')).saturating_add(10),
            _ => break,
        };
        value = value.saturating_mul(16).saturating_add(digit);
        used = used.saturating_add(1);
    }
    (u8::try_from(value).unwrap_or(u8::MAX), used)
}

fn take_hex_digits_wide(text: &str) -> (u32, usize) {
    let mut value = 0u32;
    let mut used = 0usize;
    for byte in text.as_bytes() {
        let digit = match byte {
            b'0'..=b'9' => u32::from(byte.saturating_sub(b'0')),
            b'a'..=b'f' => u32::from(byte.saturating_sub(b'a')).saturating_add(10),
            b'A'..=b'F' => u32::from(byte.saturating_sub(b'A')).saturating_add(10),
            _ => break,
        };
        value = value.saturating_mul(16).saturating_add(digit);
        used = used.saturating_add(1);
        if used >= 8 {
            break;
        }
    }
    (value, used)
}

fn valid_target(target: &str) -> bool {
    let mut characters = target.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    target.chars().all(|ch| {
        ch.is_alphanumeric() || matches!(ch, '_' | '.' | '[' | ']' | '"' | '\'' | ':' | ' ' | '\\')
    })
}
