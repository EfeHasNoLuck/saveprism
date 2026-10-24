//! The data-only pickle machine: runs the opcodes on a stack and builds an object graph.
//!
//! Nothing is imported, looked up, constructed or called. `GLOBAL` / `STACK_GLOBAL` give a class
//! name; `REDUCE`, `NEWOBJ`, `NEWOBJ_EX`, `INST` and `OBJ` give an [`Instance`] that records the
//! class, the arguments and whatever `BUILD`, `APPEND(S)`, `SETITEM(S)` and `ADDITEMS` later add.

use std::collections::HashMap;

use saveprism_core::limits::len_u64;
use saveprism_core::{ByteSpan, Error, Limits, Result, codes};

use super::ops::{self, Op, Reader, parse_error};
use super::{PICKLE_BAD_MEMO, PICKLE_UNSUPPORTED};

/// Index of an object in the graph.
pub(crate) type ObjId = u32;

/// Deepest `MARK` nesting accepted.
const MAX_MARKS: usize = 4096;

/// How a text came to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextKind {
    /// A Python 3 `str` / Python 2 `unicode`.
    Unicode,
    /// A Python 2 byte string that is valid UTF-8.
    ByteStr,
}

/// How bytes came to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BytesKind {
    /// Python 3 `bytes`.
    Bytes,
    /// Python `bytearray`.
    ByteArray,
    /// A Python 2 byte string that is not UTF-8.
    ByteStr,
}

/// How an instance was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum How {
    Reduce,
    NewObj,
    NewObjEx,
    Inst,
    Obj,
}

/// An object made by calling or instantiating something.
#[derive(Clone, Debug)]
pub(crate) struct Instance {
    pub how: How,
    /// The callable (`REDUCE`) or the class.
    pub class: ObjId,
    /// The argument tuple, when there is one.
    pub args: Option<ObjId>,
    /// The keyword arguments of `NEWOBJ_EX`.
    pub kwargs: Option<ObjId>,
    /// Every `BUILD` state, in order.
    pub states: Vec<ObjId>,
    /// `APPEND` / `APPENDS` items.
    pub items: Vec<ObjId>,
    /// `SETITEM` / `SETITEMS` entries.
    pub entries: Vec<(ObjId, ObjId)>,
    /// `ADDITEMS` members.
    pub members: Vec<ObjId>,
}

/// One object of the graph.
#[derive(Clone, Debug)]
pub(crate) enum Obj {
    None,
    Bool(bool, ByteSpan),
    Int(i128, ByteSpan),
    /// An integer beyond `i128`, as decimal text (or `None` when too long to convert).
    BigInt(Option<String>),
    Float(f64, ByteSpan),
    /// Text; `editable` is false for text that cannot be written back the same way.
    Str {
        text: String,
        kind: TextKind,
        span: ByteSpan,
        editable: bool,
    },
    Bytes(Vec<u8>, BytesKind),
    Tuple(Vec<ObjId>),
    List(Vec<ObjId>),
    Dict(Vec<(ObjId, ObjId)>),
    Set(Vec<ObjId>),
    FrozenSet(Vec<ObjId>),
    Global {
        module: String,
        name: String,
    },
    Instance(Box<Instance>),
    /// A persistent id given as text (`PERSID`) or as an object (`BINPERSID`).
    PersIdText(String),
    PersId(ObjId),
    /// An extension registry code.
    Ext(u32),
}

/// The finished graph.
pub(crate) struct Graph {
    pub objects: Vec<Obj>,
    pub root: ObjId,
    /// The protocol of the `PROTO` opcode, 0 when there is none.
    pub proto: u8,
}

impl Graph {
    pub(crate) fn get(&self, id: ObjId) -> Option<&Obj> {
        self.objects.get(id as usize)
    }
}

fn span(op: &Op) -> ByteSpan {
    ByteSpan {
        start: len_u64(op.start),
        end: len_u64(op.end),
    }
}

fn unsupported(message: impl Into<String>, offset: usize) -> Error {
    Error::new(PICKLE_UNSUPPORTED, message).with_offset(len_u64(offset))
}

/// Two's complement little-endian bytes as `i128`, `None` when the value does not fit.
pub(crate) fn le_signed(bytes: &[u8]) -> Option<i128> {
    let Some(&last) = bytes.last() else {
        return Some(0);
    };
    let negative = last & 0x80 != 0;
    let fill = if negative { 0xFF } else { 0x00 };
    if bytes.len() > 16 {
        let (low, high) = bytes.split_at(16);
        if high.iter().any(|byte| *byte != fill) {
            return None;
        }
        let top = low.last().copied().unwrap_or(0);
        if (top & 0x80 != 0) != negative {
            return None;
        }
        return le_signed(low);
    }
    let mut raw = [fill; 16];
    for (slot, byte) in raw.iter_mut().zip(bytes) {
        *slot = *byte;
    }
    Some(i128::from_le_bytes(raw))
}

/// Largest big integer (in bytes) shown as decimal text.
const MAX_BIG_DECIMAL_BYTES: usize = 4096;

/// Two's complement little-endian bytes as decimal text (any size up to the cap).
pub(crate) fn le_signed_decimal(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_BIG_DECIMAL_BYTES {
        return None;
    }
    let negative = bytes.last().is_some_and(|byte| byte & 0x80 != 0);
    // Magnitude as little-endian bytes.
    let mut magnitude: Vec<u8> = bytes.to_vec();
    if negative {
        let mut carry = true;
        for byte in &mut magnitude {
            *byte = !*byte;
            if carry {
                let (sum, overflow) = byte.overflowing_add(1);
                *byte = sum;
                carry = overflow;
            }
        }
    }
    // Big-endian u32 limbs, divided by 10^9 repeatedly.
    let mut limbs: Vec<u32> = magnitude
        .chunks(4)
        .rev()
        .map(|chunk| {
            chunk
                .iter()
                .rev()
                .fold(0_u32, |acc, byte| (acc << 8) | u32::from(*byte))
        })
        .collect();
    let mut groups: Vec<u32> = Vec::new();
    while limbs.iter().any(|limb| *limb != 0) {
        let mut remainder = 0_u64;
        for limb in &mut limbs {
            let current = (remainder << 32) | u64::from(*limb);
            *limb = u32::try_from(current / 1_000_000_000).unwrap_or(u32::MAX);
            remainder = current % 1_000_000_000;
        }
        groups.push(u32::try_from(remainder).unwrap_or(0));
    }
    let mut text = String::new();
    if negative {
        text.push('-');
    }
    match groups.split_last() {
        None => text.push('0'),
        Some((first, rest)) => {
            text.push_str(&first.to_string());
            for group in rest.iter().rev() {
                text.push_str(&format!("{group:09}"));
            }
        }
    }
    Some(text)
}

/// Parses a decimal integer line (optional sign, digits).
fn decimal(text: &[u8]) -> Option<DecimalInt> {
    let text = std::str::from_utf8(text).ok()?.trim();
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(match text.parse::<i128>() {
        Ok(value) => DecimalInt::Small(value),
        Err(_) => DecimalInt::Big(text.trim_start_matches('+').to_string()),
    })
}

enum DecimalInt {
    Small(i128),
    Big(String),
}

/// Decodes Python's `raw-unicode-escape` text (protocol 0 `UNICODE`): bytes are Latin-1
/// characters except `\uXXXX` and `\UXXXXXXXX`. `None` for a bad escape; the flag is false when
/// the text holds a surrogate (kept as U+FFFD).
pub(crate) fn raw_unicode_escape(bytes: &[u8]) -> Option<(String, bool)> {
    let mut out = String::with_capacity(bytes.len());
    let mut exact = true;
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        index += 1;
        if byte != b'\\' {
            out.push(char::from(byte));
            continue;
        }
        let Some(&next) = bytes.get(index) else {
            out.push('\\');
            continue;
        };
        let count = match next {
            b'u' => 4,
            b'U' => 8,
            _ => {
                out.push('\\');
                out.push(char::from(next));
                index += 1;
                continue;
            }
        };
        let hex = bytes.get(index + 1..index + 1 + count)?;
        let text = std::str::from_utf8(hex).ok()?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let code = u32::from_str_radix(text, 16).ok()?;
        if code > 0x10_FFFF {
            return None;
        }
        match char::from_u32(code) {
            Some(c) => out.push(c),
            None => {
                exact = false;
                out.push('\u{FFFD}');
            }
        }
        index += 1 + count;
    }
    Some((out, exact))
}

/// Decodes the quoted text of a protocol 0 `STRING` (Python's `repr` of a byte string).
fn quoted_string(arg: &[u8]) -> Option<Vec<u8>> {
    let first = *arg.first()?;
    if arg.len() < 2 || (first != b'\'' && first != b'"') || arg.last() != Some(&first) {
        return None;
    }
    let body = arg.get(1..arg.len() - 1)?;
    let mut out = Vec::with_capacity(body.len());
    let mut index = 0;
    while let Some(&byte) = body.get(index) {
        index += 1;
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(&next) = body.get(index) else {
            out.push(b'\\');
            continue;
        };
        index += 1;
        match next {
            b'\n' => {}
            b'\\' | b'\'' | b'"' => out.push(next),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0C),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0B),
            b'x' => {
                let hex = std::str::from_utf8(body.get(index..index + 2)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 2;
            }
            b'0'..=b'7' => {
                let mut value = u32::from(next - b'0');
                for _ in 0..2 {
                    match body.get(index) {
                        Some(&digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            index += 1;
                        }
                        _ => break,
                    }
                }
                out.push(u8::try_from(value & 0xFF).unwrap_or(0));
            }
            other => {
                out.push(b'\\');
                out.push(other);
            }
        }
    }
    Some(out)
}

struct Machine<'a> {
    objects: Vec<Obj>,
    stack: Vec<ObjId>,
    metastack: Vec<Vec<ObjId>>,
    memo: HashMap<u64, ObjId>,
    limits: &'a Limits,
    proto: u8,
}

impl Machine<'_> {
    fn add(&mut self, obj: Obj, at: usize) -> Result<ObjId> {
        let count = len_u64(self.objects.len());
        if count >= self.limits.max_nodes {
            return Err(Error::new(
                codes::CORE_LIMIT_NODES,
                format!(
                    "the pickle has more than {} objects (limit)",
                    self.limits.max_nodes
                ),
            )
            .with_offset(len_u64(at)));
        }
        let id = ObjId::try_from(self.objects.len())
            .map_err(|_| parse_error("the pickle has too many objects", at))?;
        self.objects.push(obj);
        Ok(id)
    }

    fn push(&mut self, obj: Obj, at: usize) -> Result<()> {
        let id = self.add(obj, at)?;
        self.stack.push(id);
        Ok(())
    }

    fn pop(&mut self, at: usize) -> Result<ObjId> {
        self.stack
            .pop()
            .ok_or_else(|| parse_error("the stack is empty", at))
    }

    fn top(&self, at: usize) -> Result<ObjId> {
        self.stack
            .last()
            .copied()
            .ok_or_else(|| parse_error("the stack is empty", at))
    }

    fn pop_mark(&mut self, at: usize) -> Result<Vec<ObjId>> {
        let outer = self
            .metastack
            .pop()
            .ok_or_else(|| parse_error("no MARK to pop", at))?;
        Ok(std::mem::replace(&mut self.stack, outer))
    }

    fn obj_mut(&mut self, id: ObjId, at: usize) -> Result<&mut Obj> {
        self.objects
            .get_mut(id as usize)
            .ok_or_else(|| parse_error("an object reference is out of range", at))
    }

    fn text_of(&self, id: ObjId) -> Option<String> {
        match self.objects.get(id as usize)? {
            Obj::Str { text, .. } => Some(text.clone()),
            _ => None,
        }
    }

    fn memo_get(&self, key: u64, at: usize) -> Result<ObjId> {
        self.memo.get(&key).copied().ok_or_else(|| {
            Error::new(PICKLE_BAD_MEMO, format!("memo entry {key} is not defined"))
                .with_offset(len_u64(at))
        })
    }

    fn memo_put(&mut self, key: u64, at: usize) -> Result<()> {
        let top = self.top(at)?;
        self.memo.insert(key, top);
        Ok(())
    }

    fn append(&mut self, target: ObjId, items: Vec<ObjId>, at: usize) -> Result<()> {
        match self.obj_mut(target, at)? {
            Obj::List(list) => list.extend(items),
            Obj::Instance(instance) => instance.items.extend(items),
            _ => return Err(parse_error("APPEND to a value that is not a list", at)),
        }
        Ok(())
    }

    fn set_items(&mut self, target: ObjId, pairs: Vec<(ObjId, ObjId)>, at: usize) -> Result<()> {
        match self.obj_mut(target, at)? {
            Obj::Dict(entries) => entries.extend(pairs),
            Obj::Instance(instance) => instance.entries.extend(pairs),
            _ => return Err(parse_error("SETITEM on a value that is not a dict", at)),
        }
        Ok(())
    }

    fn instance(&mut self, how: How, class: ObjId, args: Option<ObjId>, at: usize) -> Result<()> {
        self.push(
            Obj::Instance(Box::new(Instance {
                how,
                class,
                args,
                kwargs: None,
                states: Vec::new(),
                items: Vec::new(),
                entries: Vec::new(),
                members: Vec::new(),
            })),
            at,
        )
    }

    /// UTF-8 text; text Python wrote with lone surrogates (not UTF-8) is kept lossy, not editable.
    fn text_arg(bytes: &[u8]) -> (String, bool) {
        match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_string(), true),
            Err(_) => (String::from_utf8_lossy(bytes).into_owned(), false),
        }
    }

    fn step(&mut self, reader: &Reader<'_>, op: &Op) -> Result<Option<ObjId>> {
        let at = op.start;
        let arg = reader.arg(op);
        match op.code {
            ops::PROTO => {
                let version = arg.first().copied().unwrap_or(0);
                if version > 5 {
                    return Err(unsupported(
                        format!("pickle protocol {version} is not supported (0 to 5 are)"),
                        at,
                    ));
                }
                if self.proto == 0 {
                    self.proto = version;
                }
            }
            ops::FRAME => {}
            ops::STOP => return Ok(Some(self.pop(at)?)),
            ops::MARK => {
                if self.metastack.len() >= MAX_MARKS {
                    return Err(Error::new(
                        codes::CORE_LIMIT_DEPTH,
                        format!("more than {MAX_MARKS} nested MARKs"),
                    )
                    .with_offset(len_u64(at)));
                }
                let current = std::mem::take(&mut self.stack);
                self.metastack.push(current);
            }
            ops::POP => {
                if self.stack.pop().is_none() {
                    self.pop_mark(at)?;
                }
            }
            ops::POP_MARK => {
                self.pop_mark(at)?;
            }
            ops::DUP => {
                let top = self.top(at)?;
                self.stack.push(top);
            }
            ops::NONE => self.push(Obj::None, at)?,
            ops::NEWTRUE => self.push(Obj::Bool(true, span(op)), at)?,
            ops::NEWFALSE => self.push(Obj::Bool(false, span(op)), at)?,
            ops::INT => {
                let text = ops::line(arg);
                let obj = match text {
                    b"00" => Obj::Bool(false, span(op)),
                    b"01" => Obj::Bool(true, span(op)),
                    _ => match decimal(text) {
                        Some(DecimalInt::Small(value)) => Obj::Int(value, span(op)),
                        Some(DecimalInt::Big(text)) => Obj::BigInt(Some(text)),
                        None => return Err(parse_error("an INT line is not a number", at)),
                    },
                };
                self.push(obj, at)?;
            }
            ops::LONG => {
                let text = ops::line(arg);
                let text = text.strip_suffix(b"L").unwrap_or(text);
                let obj = match decimal(text) {
                    Some(DecimalInt::Small(value)) => Obj::Int(value, span(op)),
                    Some(DecimalInt::Big(text)) => Obj::BigInt(Some(text)),
                    None => return Err(parse_error("a LONG line is not a number", at)),
                };
                self.push(obj, at)?;
            }
            ops::BININT1 | ops::BININT2 => {
                let value = i128::from(ops::le_u64(arg));
                self.push(Obj::Int(value, span(op)), at)?;
            }
            ops::BININT => {
                let raw =
                    <[u8; 4]>::try_from(arg).map_err(|_| parse_error("BININT is cut short", at))?;
                self.push(Obj::Int(i128::from(i32::from_le_bytes(raw)), span(op)), at)?;
            }
            ops::LONG1 | ops::LONG4 => {
                let obj = match le_signed(arg) {
                    Some(value) => Obj::Int(value, span(op)),
                    None => Obj::BigInt(le_signed_decimal(arg)),
                };
                self.push(obj, at)?;
            }
            ops::FLOAT => {
                let text = std::str::from_utf8(ops::line(arg))
                    .map_err(|_| parse_error("a FLOAT line is not text", at))?;
                let value = text
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| parse_error("a FLOAT line is not a number", at))?;
                self.push(Obj::Float(value, span(op)), at)?;
            }
            ops::BINFLOAT => {
                let raw = <[u8; 8]>::try_from(arg)
                    .map_err(|_| parse_error("BINFLOAT is cut short", at))?;
                self.push(Obj::Float(f64::from_be_bytes(raw), span(op)), at)?;
            }
            ops::SHORT_BINUNICODE | ops::BINUNICODE | ops::BINUNICODE8 => {
                let (text, exact) = Self::text_arg(arg);
                self.push(
                    Obj::Str {
                        text,
                        kind: TextKind::Unicode,
                        span: span(op),
                        editable: exact,
                    },
                    at,
                )?;
            }
            ops::UNICODE => {
                let (text, exact) = raw_unicode_escape(ops::line(arg))
                    .ok_or_else(|| parse_error("a UNICODE line has a bad escape", at))?;
                self.push(
                    Obj::Str {
                        text,
                        kind: TextKind::Unicode,
                        span: span(op),
                        editable: exact,
                    },
                    at,
                )?;
            }
            ops::SHORT_BINSTRING | ops::BINSTRING => {
                let obj = match std::str::from_utf8(arg) {
                    Ok(text) => Obj::Str {
                        text: text.to_string(),
                        kind: TextKind::ByteStr,
                        span: span(op),
                        editable: true,
                    },
                    Err(_) => Obj::Bytes(arg.to_vec(), BytesKind::ByteStr),
                };
                self.push(obj, at)?;
            }
            ops::STRING => {
                let bytes = quoted_string(ops::line(arg))
                    .ok_or_else(|| parse_error("a STRING line is not quoted text", at))?;
                let obj = match String::from_utf8(bytes) {
                    Ok(text) => Obj::Str {
                        text,
                        kind: TextKind::ByteStr,
                        span: span(op),
                        editable: false,
                    },
                    Err(error) => Obj::Bytes(error.into_bytes(), BytesKind::ByteStr),
                };
                self.push(obj, at)?;
            }
            ops::SHORT_BINBYTES | ops::BINBYTES | ops::BINBYTES8 => {
                self.push(Obj::Bytes(arg.to_vec(), BytesKind::Bytes), at)?;
            }
            ops::BYTEARRAY8 => self.push(Obj::Bytes(arg.to_vec(), BytesKind::ByteArray), at)?,
            ops::EMPTY_TUPLE => self.push(Obj::Tuple(Vec::new()), at)?,
            ops::TUPLE => {
                let items = self.pop_mark(at)?;
                self.push(Obj::Tuple(items), at)?;
            }
            ops::TUPLE1 | ops::TUPLE2 | ops::TUPLE3 => {
                let count = usize::from(op.code - ops::TUPLE1) + 1;
                let split = self
                    .stack
                    .len()
                    .checked_sub(count)
                    .ok_or_else(|| parse_error("the stack is too short for a tuple", at))?;
                let items = self.stack.split_off(split);
                self.push(Obj::Tuple(items), at)?;
            }
            ops::EMPTY_LIST => self.push(Obj::List(Vec::new()), at)?,
            ops::LIST => {
                let items = self.pop_mark(at)?;
                self.push(Obj::List(items), at)?;
            }
            ops::EMPTY_DICT => self.push(Obj::Dict(Vec::new()), at)?,
            ops::DICT => {
                let items = self.pop_mark(at)?;
                let pairs = pairs(items, at)?;
                self.push(Obj::Dict(pairs), at)?;
            }
            ops::EMPTY_SET => self.push(Obj::Set(Vec::new()), at)?,
            ops::FROZENSET => {
                let items = self.pop_mark(at)?;
                self.push(Obj::FrozenSet(items), at)?;
            }
            ops::ADDITEMS => {
                let items = self.pop_mark(at)?;
                let target = self.top(at)?;
                match self.obj_mut(target, at)? {
                    Obj::Set(members) => members.extend(items),
                    Obj::Instance(instance) => instance.members.extend(items),
                    _ => return Err(parse_error("ADDITEMS to a value that is not a set", at)),
                }
            }
            ops::APPEND => {
                let value = self.pop(at)?;
                let target = self.top(at)?;
                self.append(target, vec![value], at)?;
            }
            ops::APPENDS => {
                let items = self.pop_mark(at)?;
                let target = self.top(at)?;
                self.append(target, items, at)?;
            }
            ops::SETITEM => {
                let value = self.pop(at)?;
                let key = self.pop(at)?;
                let target = self.top(at)?;
                self.set_items(target, vec![(key, value)], at)?;
            }
            ops::SETITEMS => {
                let items = self.pop_mark(at)?;
                let target = self.top(at)?;
                let pairs = pairs(items, at)?;
                self.set_items(target, pairs, at)?;
            }
            ops::GLOBAL | ops::INST => {
                let (module, name) = ops::two_lines(arg)
                    .ok_or_else(|| parse_error("a class name is cut short", at))?;
                let module = String::from_utf8_lossy(module).into_owned();
                let name = String::from_utf8_lossy(name).into_owned();
                if op.code == ops::GLOBAL {
                    self.push(Obj::Global { module, name }, at)?;
                } else {
                    let args = self.pop_mark(at)?;
                    let class = self.add(Obj::Global { module, name }, at)?;
                    let args = self.add(Obj::Tuple(args), at)?;
                    self.instance(How::Inst, class, Some(args), at)?;
                }
            }
            ops::STACK_GLOBAL => {
                let name = self.pop(at)?;
                let module = self.pop(at)?;
                let (Some(module), Some(name)) = (self.text_of(module), self.text_of(name)) else {
                    return Err(parse_error("STACK_GLOBAL needs two texts", at));
                };
                self.push(Obj::Global { module, name }, at)?;
            }
            ops::OBJ => {
                let mut items = self.pop_mark(at)?;
                if items.is_empty() {
                    return Err(parse_error("OBJ without a class", at));
                }
                let class = items.remove(0);
                let args = self.add(Obj::Tuple(items), at)?;
                self.instance(How::Obj, class, Some(args), at)?;
            }
            ops::REDUCE => {
                let args = self.pop(at)?;
                let callable = self.pop(at)?;
                self.instance(How::Reduce, callable, Some(args), at)?;
            }
            ops::NEWOBJ => {
                let args = self.pop(at)?;
                let class = self.pop(at)?;
                self.instance(How::NewObj, class, Some(args), at)?;
            }
            ops::NEWOBJ_EX => {
                let kwargs = self.pop(at)?;
                let args = self.pop(at)?;
                let class = self.pop(at)?;
                self.instance(How::NewObjEx, class, Some(args), at)?;
                let top = self.top(at)?;
                if let Obj::Instance(instance) = self.obj_mut(top, at)? {
                    instance.kwargs = Some(kwargs);
                }
            }
            ops::BUILD => {
                let state = self.pop(at)?;
                let target = self.top(at)?;
                match self.obj_mut(target, at)? {
                    Obj::Instance(instance) => instance.states.push(state),
                    _ => return Err(parse_error("BUILD on a value that is not an object", at)),
                }
            }
            ops::PERSID => {
                let text = String::from_utf8_lossy(ops::line(arg)).into_owned();
                self.push(Obj::PersIdText(text), at)?;
            }
            ops::BINPERSID => {
                let id = self.pop(at)?;
                self.push(Obj::PersId(id), at)?;
            }
            ops::EXT1 | ops::EXT2 | ops::EXT4 => {
                let code = u32::try_from(ops::le_u64(arg)).unwrap_or(u32::MAX);
                self.push(Obj::Ext(code), at)?;
            }
            ops::GET => {
                let key = match decimal(ops::line(arg)) {
                    Some(DecimalInt::Small(value)) => u64::try_from(value).ok(),
                    _ => None,
                }
                .ok_or_else(|| parse_error("a GET line is not a memo index", at))?;
                let id = self.memo_get(key, at)?;
                self.stack.push(id);
            }
            ops::BINGET | ops::LONG_BINGET => {
                let id = self.memo_get(ops::le_u64(arg), at)?;
                self.stack.push(id);
            }
            ops::PUT => {
                let key = match decimal(ops::line(arg)) {
                    Some(DecimalInt::Small(value)) => u64::try_from(value).ok(),
                    _ => None,
                }
                .ok_or_else(|| parse_error("a PUT line is not a memo index", at))?;
                self.memo_put(key, at)?;
            }
            ops::BINPUT | ops::LONG_BINPUT => self.memo_put(ops::le_u64(arg), at)?,
            ops::MEMOIZE => {
                let key = len_u64(self.memo.len());
                self.memo_put(key, at)?;
            }
            ops::NEXT_BUFFER | ops::READONLY_BUFFER => {
                return Err(unsupported(
                    "out-of-band buffers (protocol 5 NEXT_BUFFER) are not supported",
                    at,
                ));
            }
            other => {
                return Err(parse_error(format!("unknown opcode 0x{other:02x}"), at));
            }
        }
        Ok(None)
    }
}

fn pairs(items: Vec<ObjId>, at: usize) -> Result<Vec<(ObjId, ObjId)>> {
    let (chunks, rest) = items.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(parse_error("a dict has a key without a value", at));
    }
    Ok(chunks.iter().map(|[key, value]| (*key, *value)).collect())
}

/// Runs the whole stream and returns the graph.
pub(crate) fn run(input: &[u8], limits: &Limits) -> Result<Graph> {
    let mut machine = Machine {
        objects: Vec::new(),
        stack: Vec::new(),
        metastack: Vec::new(),
        memo: HashMap::new(),
        limits,
        proto: 0,
    };
    let mut reader = Reader::new(input);
    while let Some(op) = reader.next_op()? {
        if let Some(root) = machine.step(&reader, &op)? {
            return Ok(Graph {
                objects: machine.objects,
                root,
                proto: machine.proto,
            });
        }
    }
    Err(parse_error(
        "the pickle ends without a STOP opcode",
        input.len(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_signed_values() {
        assert_eq!(le_signed(&[]), Some(0));
        assert_eq!(le_signed(&[0xFF]), Some(-1));
        assert_eq!(le_signed(&[0xFF, 0x00]), Some(255));
        assert_eq!(le_signed(&[0x80]), Some(-128));
        let mut big = vec![0_u8; 17];
        big[16] = 1;
        assert_eq!(le_signed(&big), None);
        let mut padded = vec![0xFF_u8; 20];
        padded[0] = 0xFE;
        assert_eq!(le_signed(&padded), Some(-2));
    }

    #[test]
    fn big_decimal_text() {
        assert_eq!(le_signed_decimal(&[]).unwrap(), "0");
        assert_eq!(le_signed_decimal(&[0xFF]).unwrap(), "-1");
        // 2**136 = 87112285931760246646623899502532662132736
        let mut bytes = vec![0_u8; 18];
        bytes[17] = 1;
        assert_eq!(
            le_signed_decimal(&bytes).unwrap(),
            "87112285931760246646623899502532662132736"
        );
        let mut negative = vec![0_u8; 18];
        negative[17] = 0xFF;
        assert_eq!(
            le_signed_decimal(&negative).unwrap(),
            "-87112285931760246646623899502532662132736"
        );
    }

    #[test]
    fn raw_unicode_escape_rules() {
        assert_eq!(
            raw_unicode_escape(b"a\\u00e9\\U0001F600\xe9").unwrap(),
            ("aé😀é".to_string(), true)
        );
        assert_eq!(
            raw_unicode_escape(b"\\\\x\\q\\").unwrap(),
            ("\\\\x\\q\\".to_string(), true)
        );
        assert!(!raw_unicode_escape(b"\\ud800").unwrap().1);
        assert!(raw_unicode_escape(b"\\u12").is_none());
        assert!(raw_unicode_escape(b"\\U00110000").is_none());
    }

    #[test]
    fn quoted_strings() {
        assert_eq!(quoted_string(b"'a\\nb\\x41\\101'").unwrap(), b"a\nbAA");
        assert_eq!(quoted_string(b"\"it's\"").unwrap(), b"it's");
        assert!(quoted_string(b"'open").is_none());
    }
}
