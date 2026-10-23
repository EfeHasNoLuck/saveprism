//! Writes a document back as Marshal data: canonical form plus what `read` recorded.

use std::collections::{HashMap, HashSet};

use saveprism_core::{Error, Node, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc, Value};

use super::{Enc, Kind, MAX_NESTING, RUBY_WRITE, VERSION, parse_hint};

/// Smallest and largest integer Ruby writes as a Fixnum (`i`); everything else is a Bignum (`l`).
const FIXNUM_MIN: i128 = -(1 << 30);
const FIXNUM_MAX: i128 = (1 << 30) - 1;
/// Prefix of the lexeme that keeps the original bytes of a non-canonical integer.
pub(super) const RAW_PREFIX: &str = "raw:";

pub(super) const P_SYMBOL_REDEFS: &str = "symbol_redefs";
pub(super) const P_SYMBOL_LINKS: &str = "symbol_links";
pub(super) const P_SYMBOL_UTF8: &str = "symbol_utf8";
pub(super) const P_SYMBOL_ASCII: &str = "symbol_ascii";
pub(super) const P_FLOAT_TAILS: &str = "float_tails";

fn write_error(message: impl Into<String>) -> Error {
    Error::new(RUBY_WRITE, message)
}

/// Appends a Marshal "long" (the packed integer used for Fixnums, lengths and indices).
/// Fails above 4 bytes of precision (`-2^32 ..= 2^32 - 1`).
pub(super) fn push_long(out: &mut Vec<u8>, value: i64) -> Result<()> {
    if value == 0 {
        out.push(0);
        return Ok(());
    }
    if (1..123).contains(&value) {
        out.push(u8::try_from(value + 5).unwrap_or(0));
        return Ok(());
    }
    if (-123..0).contains(&value) {
        out.push(((value - 5) & 0xff) as u8);
        return Ok(());
    }
    let mut bytes = [0_u8; 4];
    let mut rest = value;
    for count in 1..=4_u8 {
        if let Some(slot) = bytes.get_mut(usize::from(count - 1)) {
            *slot = (rest & 0xff) as u8;
        }
        rest >>= 8;
        let done = if value >= 0 { rest == 0 } else { rest == -1 };
        if done {
            let head = if value >= 0 {
                count
            } else {
                count.wrapping_neg()
            };
            out.push(head);
            out.extend_from_slice(bytes.get(..usize::from(count)).unwrap_or(&[]));
            return Ok(());
        }
    }
    Err(write_error(format!(
        "the number {value} is too large for a Marshal length or index"
    )))
}

fn long_of(value: usize) -> Result<i64> {
    i64::try_from(value).map_err(|_| write_error("a length is too large to write"))
}

/// The canonical bytes of an integer (type byte included): `i` for the Fixnum range, else `l`.
pub(super) fn int_bytes(value: i128) -> Vec<u8> {
    let mut out = Vec::new();
    if (FIXNUM_MIN..=FIXNUM_MAX).contains(&value) {
        out.push(b'i');
        // In the Fixnum range the value always fits a long.
        let _ = push_long(&mut out, i64::try_from(value).unwrap_or(0));
        return out;
    }
    out.push(b'l');
    out.push(if value < 0 { b'-' } else { b'+' });
    let magnitude = value.unsigned_abs().to_le_bytes();
    let used = magnitude
        .iter()
        .rposition(|byte| *byte != 0)
        .map_or(0, |last| last + 1);
    let shorts = used.div_ceil(2);
    // At most 8 shorts: always fits.
    let _ = push_long(&mut out, i64::try_from(shorts).unwrap_or(0));
    out.extend_from_slice(magnitude.get(..shorts * 2).unwrap_or(&[]));
    out
}

/// The text Ruby (1.9 and later) writes for a float: the shortest digits that read back to the
/// same value, as plain decimals or as `<d>.<ddd>e<exp>` when the exponent is far from the digits.
pub(super) fn float_text(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_string();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if value == 0.0 {
        return format!("{sign}0");
    }
    // `{:e}` gives the shortest round-trip digits: "1.25e2".
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i64 = exponent.parse().unwrap_or(0);
    let count = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    let point = exponent + 1;
    let mut text = String::from(sign);
    if point < -3 || point > count {
        let (first, rest) = digits.split_at(digits.len().min(1));
        text.push_str(first);
        if !rest.is_empty() {
            text.push('.');
            text.push_str(rest);
        }
        text.push_str(&format!("e{}", point - 1));
    } else if point > 0 {
        let split = usize::try_from(point).unwrap_or(0).min(digits.len());
        let (whole, fraction) = digits.split_at(split);
        text.push_str(whole);
        if !fraction.is_empty() {
            text.push('.');
            text.push_str(fraction);
        }
    } else {
        text.push_str("0.");
        for _ in 0..point.unsigned_abs() {
            text.push('0');
        }
        text.push_str(&digits);
    }
    text
}

/// Lowercase hex of `bytes`.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| {
            text.get(index..index + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

/// What `read` recorded in the parameters.
#[derive(Default)]
struct Recorded<'p> {
    redefs: HashSet<i64>,
    links: HashMap<i64, i64>,
    utf8: HashSet<i64>,
    ascii: HashSet<i64>,
    tails: HashMap<i64, &'p [u8]>,
}

fn int_list(params: &Params, key: &str) -> Result<HashSet<i64>> {
    match params.get(key) {
        None => Ok(HashSet::new()),
        Some(ParamValue::List(items)) => items
            .iter()
            .map(|item| match item {
                ParamValue::Int(value) => Ok(*value),
                _ => Err(bad_param(key)),
            })
            .collect(),
        Some(_) => Err(bad_param(key)),
    }
}

fn bad_param(key: &str) -> Error {
    Error::new(
        saveprism_core::codes::CORE_INVALID_PARAM,
        format!("parameter \"{key}\" of format.ruby-marshal has the wrong shape"),
    )
}

impl<'p> Recorded<'p> {
    fn from_params(params: &'p Params) -> Result<Self> {
        let mut links = HashMap::new();
        match params.get(P_SYMBOL_LINKS) {
            None => {}
            Some(ParamValue::Table(table)) => {
                for (key, value) in table.iter() {
                    let (Ok(ordinal), ParamValue::Int(index)) = (key.parse::<i64>(), value) else {
                        return Err(bad_param(P_SYMBOL_LINKS));
                    };
                    links.insert(ordinal, *index);
                }
            }
            Some(_) => return Err(bad_param(P_SYMBOL_LINKS)),
        }
        let mut tails = HashMap::new();
        match params.get(P_FLOAT_TAILS) {
            None => {}
            Some(ParamValue::Table(table)) => {
                for (key, value) in table.iter() {
                    let (Ok(ordinal), ParamValue::Bytes(bytes)) = (key.parse::<i64>(), value)
                    else {
                        return Err(bad_param(P_FLOAT_TAILS));
                    };
                    tails.insert(ordinal, bytes.as_slice());
                }
            }
            Some(_) => return Err(bad_param(P_FLOAT_TAILS)),
        }
        Ok(Self {
            redefs: int_list(params, P_SYMBOL_REDEFS)?,
            links,
            utf8: int_list(params, P_SYMBOL_UTF8)?,
            ascii: int_list(params, P_SYMBOL_ASCII)?,
            tails,
        })
    }
}

/// Writes the whole document.
pub(super) fn encode(doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
    let recorded = Recorded::from_params(params)?;
    let mut writer = Writer {
        doc,
        out: Vec::new(),
        symbols: Vec::new(),
        first_symbol: HashMap::new(),
        objects: HashMap::new(),
        written: vec![false; doc.len()],
        symbol_ordinal: 0,
        float_ordinal: 0,
        recorded,
    };
    writer.write_root()?;
    Ok(writer.out)
}

struct Writer<'d, 'p> {
    doc: &'d SaveDoc,
    out: Vec<u8>,
    /// Symbol table of the current dump.
    symbols: Vec<&'d str>,
    first_symbol: HashMap<&'d str, usize>,
    /// Object table of the current dump: node → index.
    objects: HashMap<NodeId, usize>,
    /// Nodes already written (a node reached twice without a link is refused).
    written: Vec<bool>,
    symbol_ordinal: i64,
    float_ordinal: i64,
    recorded: Recorded<'p>,
}

impl<'d> Writer<'d, '_> {
    fn node(&self, id: NodeId) -> Result<&'d Node> {
        self.doc
            .get(id)
            .ok_or_else(|| write_error(format!("node {} is missing", id.index())))
    }

    fn kind(&self, id: NodeId) -> Result<Kind<'d>> {
        let node = self.node(id)?;
        node.type_hint
            .as_ref()
            .and_then(|hint| parse_hint(hint.as_str()))
            .ok_or_else(|| {
                write_error(format!(
                    "node {} has no Ruby type; values can be changed but not added",
                    id.index()
                ))
            })
    }

    fn long(&mut self, value: i64) -> Result<()> {
        push_long(&mut self.out, value)
    }

    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.long(long_of(bytes.len())?)?;
        self.out.extend_from_slice(bytes);
        Ok(())
    }

    fn register(&mut self, id: NodeId) -> Result<()> {
        let index = self.objects.len();
        if self.objects.insert(id, index).is_some() {
            return Err(write_error(format!(
                "node {} is written twice without a link",
                id.index()
            )));
        }
        Ok(())
    }

    fn write_root(&mut self) -> Result<()> {
        let root = self.doc.root();
        if self.kind(root)? != Kind::Dumps {
            return Err(write_error("the root is not a list of Marshal dumps"));
        }
        let Value::Array(dumps) = &self.node(root)?.value else {
            return Err(write_error("the root is not a list of Marshal dumps"));
        };
        let count = dumps.len();
        for (index, dump) in dumps.iter().enumerate() {
            if self.kind(*dump)? == Kind::Trailing {
                if index + 1 != count {
                    return Err(write_error("trailing bytes must come after the last dump"));
                }
                let Value::Bytes(bytes) = &self.node(*dump)?.value else {
                    return Err(write_error("the trailing bytes changed type"));
                };
                self.out.extend_from_slice(bytes);
                continue;
            }
            self.symbols.clear();
            self.first_symbol.clear();
            self.objects.clear();
            self.out.extend_from_slice(&VERSION);
            self.value(*dump, 1, None)?;
        }
        Ok(())
    }

    fn symbol(&mut self, name: &'d str, depth: usize) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(write_error("the document is nested too deeply"));
        }
        let ordinal = self.symbol_ordinal;
        self.symbol_ordinal += 1;
        let known = self.first_symbol.get(name).copied();
        if let Some(first) = known
            && !self.recorded.redefs.contains(&ordinal)
        {
            let index = match self.recorded.links.get(&ordinal) {
                Some(index) => usize::try_from(*index).unwrap_or(usize::MAX),
                None => first,
            };
            if self.symbols.get(index) != Some(&name) {
                return Err(write_error(format!(
                    "symbol link {index} does not name :{name}"
                )));
            }
            self.out.push(b';');
            return self.long(long_of(index)?);
        }
        let flag = if self.recorded.utf8.contains(&ordinal) {
            Some(true)
        } else if self.recorded.ascii.contains(&ordinal) {
            Some(false)
        } else {
            None
        };
        if flag.is_some() {
            self.out.push(b'I');
        }
        self.out.push(b':');
        self.bytes(name.as_bytes())?;
        self.first_symbol.entry(name).or_insert(self.symbols.len());
        self.symbols.push(name);
        if let Some(flag) = flag {
            self.encoding_ivar(flag, depth)?;
        }
        Ok(())
    }

    /// `1, :E, true|false`: the one instance variable of a UTF-8 or US-ASCII string.
    fn encoding_ivar(&mut self, utf8: bool, depth: usize) -> Result<()> {
        self.long(1)?;
        self.symbol("E", depth + 1)?;
        self.out.push(if utf8 { b'T' } else { b'F' });
        Ok(())
    }

    fn mark_written(&mut self, id: NodeId) -> Result<()> {
        match self.written.get_mut(id.index()) {
            Some(slot) if !*slot => {
                *slot = true;
                Ok(())
            }
            Some(_) => Err(write_error(format!(
                "node {} is reached twice without a link",
                id.index()
            ))),
            None => Err(write_error(format!("node {} is missing", id.index()))),
        }
    }

    /// Writes the value at `id`. `deferred` is set by an `I` wrapper: a `_dump` object inside it
    /// is numbered after the wrapper's instance variables, so its id is handed back instead.
    /// Every case is its own small method: the recursion goes through here once per nesting
    /// level, so this frame stays small.
    fn value(
        &mut self,
        id: NodeId,
        depth: usize,
        deferred: Option<&mut Option<NodeId>>,
    ) -> Result<()> {
        if depth > MAX_NESTING {
            return Err(write_error("the document is nested too deeply"));
        }
        let kind = self.kind(id)?;
        let node = self.node(id)?;
        if kind != Kind::Link {
            self.mark_written(id)?;
        }
        match (kind, &node.value) {
            (Kind::Nil, Value::Null) => self.out.push(b'0'),
            (Kind::Bool, Value::Bool(value)) => self.out.push(if *value { b'T' } else { b'F' }),
            (Kind::Int, Value::Int { v, .. }) => self.int(id, node, *v)?,
            (Kind::Bignum, Value::Bytes(payload)) => {
                self.register(id)?;
                self.out.push(b'l');
                self.out.extend_from_slice(payload);
            }
            (Kind::Float, Value::Float { v, .. }) => self.float(id, node, *v)?,
            (Kind::Str(enc), Value::Str(_) | Value::Bytes(_)) => {
                self.string(id, node, enc, depth)?
            }
            (Kind::Symbol, Value::Str(name)) => self.symbol(name, depth)?,
            (Kind::Regexp(enc, options), Value::Str(_) | Value::Bytes(_)) => {
                self.regexp(id, node, enc, options, depth)?;
            }
            (Kind::Class | Kind::Module | Kind::OldModule, Value::Str(_) | Value::Bytes(_)) => {
                self.class_ref(id, node, kind)?;
            }
            (Kind::UserDef(enc, class), Value::Bytes(data)) => {
                self.user_def(id, enc, class, data, depth, deferred)?;
            }
            (Kind::Array, Value::Array(items)) => self.array(id, items, depth)?,
            (Kind::Hash | Kind::HashDefault, Value::Object(_) | Value::Map(_)) => {
                self.hash(id, kind == Kind::HashDefault, depth)?;
            }
            (Kind::Object(class) | Kind::Struct(class), Value::Object(entries)) => {
                let tag = if matches!(kind, Kind::Object(_)) {
                    b'o'
                } else {
                    b'S'
                };
                self.object(id, tag, class, entries, depth)?;
            }
            (Kind::UserMarshal(class) | Kind::Data(class), Value::Object(entries)) => {
                let tag = if matches!(kind, Kind::UserMarshal(_)) {
                    b'U'
                } else {
                    b'd'
                };
                self.with_data(id, tag, class, entries, depth)?;
            }
            (Kind::UserClass(name) | Kind::Extended(name), Value::Object(entries)) => {
                let tag = if matches!(kind, Kind::UserClass(_)) {
                    b'C'
                } else {
                    b'e'
                };
                self.wrapper(id, tag, name, entries, depth, deferred)?;
            }
            (Kind::Ivars, Value::Object(entries)) => self.ivars(id, entries, depth, deferred)?,
            (Kind::Link, Value::Ref(target)) => self.link(id, *target)?,
            _ => return Err(changed(id)),
        }
        Ok(())
    }

    fn int(&mut self, id: NodeId, node: &Node, value: i128) -> Result<()> {
        let raw = node
            .lexeme
            .as_deref()
            .and_then(|lexeme| lexeme.strip_prefix(RAW_PREFIX))
            .and_then(unhex);
        let bytes = raw.unwrap_or_else(|| int_bytes(value));
        if bytes.first() == Some(&b'l') {
            self.register(id)?;
        }
        self.out.extend_from_slice(&bytes);
        Ok(())
    }

    fn float(&mut self, id: NodeId, node: &Node, value: f64) -> Result<()> {
        self.register(id)?;
        let ordinal = self.float_ordinal;
        self.float_ordinal += 1;
        self.out.push(b'f');
        match node.lexeme.as_deref() {
            Some(text) => {
                let tail = self.recorded.tails.get(&ordinal).copied().unwrap_or(&[]);
                let mut bytes = text.as_bytes().to_vec();
                bytes.extend_from_slice(tail);
                self.bytes(&bytes)
            }
            None => self.bytes(float_text(value).as_bytes()),
        }
    }

    fn string(&mut self, id: NodeId, node: &Node, enc: Enc, depth: usize) -> Result<()> {
        let bytes = text_bytes(&node.value).ok_or_else(|| changed(id))?;
        let edited = !node.flags.contains(NodeFlags::READ_ONLY);
        if enc == Enc::Ascii && edited && !bytes.is_ascii() {
            return Err(write_error(format!(
                "the text at node {} must stay ASCII (it is a US-ASCII string)",
                id.index()
            )));
        }
        // A read-only string is written as it was read, even when its bytes are not valid text.
        if enc == Enc::Utf8 && edited && std::str::from_utf8(bytes).is_err() {
            return Err(changed(id));
        }
        self.register(id)?;
        if enc != Enc::Binary {
            self.out.push(b'I');
        }
        self.out.push(b'"');
        self.bytes(bytes)?;
        self.encoding_of(enc, depth)
    }

    fn regexp(
        &mut self,
        id: NodeId,
        node: &Node,
        enc: Enc,
        options: u8,
        depth: usize,
    ) -> Result<()> {
        let source = text_bytes(&node.value).ok_or_else(|| changed(id))?;
        self.register(id)?;
        if enc != Enc::Binary {
            self.out.push(b'I');
        }
        self.out.push(b'/');
        self.bytes(source)?;
        self.out.push(options);
        self.encoding_of(enc, depth)
    }

    fn class_ref(&mut self, id: NodeId, node: &Node, kind: Kind<'_>) -> Result<()> {
        let path = text_bytes(&node.value).ok_or_else(|| changed(id))?;
        self.register(id)?;
        self.out.push(match kind {
            Kind::Class => b'c',
            Kind::Module => b'm',
            _ => b'M',
        });
        self.bytes(path)
    }

    fn user_def(
        &mut self,
        id: NodeId,
        enc: Enc,
        class: &'d str,
        data: &[u8],
        depth: usize,
        deferred: Option<&mut Option<NodeId>>,
    ) -> Result<()> {
        if enc != Enc::Binary {
            self.out.push(b'I');
        }
        self.out.push(b'u');
        self.symbol(class, depth + 1)?;
        self.bytes(data)?;
        self.encoding_of(enc, depth)?;
        match deferred {
            Some(slot) => {
                *slot = Some(id);
                Ok(())
            }
            None => self.register(id),
        }
    }

    fn array(&mut self, id: NodeId, items: &[NodeId], depth: usize) -> Result<()> {
        self.register(id)?;
        self.out.push(b'[');
        self.long(long_of(items.len())?)?;
        for item in items {
            self.value(*item, depth + 1, None)?;
        }
        Ok(())
    }

    fn object(
        &mut self,
        id: NodeId,
        tag: u8,
        class: &'d str,
        entries: &'d [(String, NodeId)],
        depth: usize,
    ) -> Result<()> {
        self.register(id)?;
        self.out.push(tag);
        self.symbol(class, depth + 1)?;
        self.long(long_of(entries.len())?)?;
        for (name, child) in entries {
            self.symbol(name, depth + 1)?;
            self.value(*child, depth + 1, None)?;
        }
        Ok(())
    }

    fn with_data(
        &mut self,
        id: NodeId,
        tag: u8,
        class: &'d str,
        entries: &[(String, NodeId)],
        depth: usize,
    ) -> Result<()> {
        let [(_, data)] = entries else {
            return Err(changed(id));
        };
        self.register(id)?;
        self.out.push(tag);
        self.symbol(class, depth + 1)?;
        self.value(*data, depth + 1, None)
    }

    fn wrapper(
        &mut self,
        id: NodeId,
        tag: u8,
        name: &'d str,
        entries: &[(String, NodeId)],
        depth: usize,
        deferred: Option<&mut Option<NodeId>>,
    ) -> Result<()> {
        let [(_, inner)] = entries else {
            return Err(changed(id));
        };
        self.out.push(tag);
        self.symbol(name, depth + 1)?;
        self.value(*inner, depth + 1, deferred)
    }

    fn ivars(
        &mut self,
        id: NodeId,
        entries: &'d [(String, NodeId)],
        depth: usize,
        deferred: Option<&mut Option<NodeId>>,
    ) -> Result<()> {
        let Some(((_, inner), ivars)) = entries.split_first() else {
            return Err(changed(id));
        };
        self.out.push(b'I');
        let mut late = None;
        self.value(*inner, depth + 1, Some(&mut late))?;
        self.long(long_of(ivars.len())?)?;
        for (name, child) in ivars {
            self.symbol(name, depth + 1)?;
            self.value(*child, depth + 1, None)?;
        }
        match (late, deferred) {
            (Some(late), Some(slot)) => *slot = Some(late),
            (Some(late), None) => self.register(late)?,
            (None, _) => {}
        }
        Ok(())
    }

    fn link(&mut self, id: NodeId, target: NodeId) -> Result<()> {
        let index = self.objects.get(&target).copied().ok_or_else(|| {
            write_error(format!(
                "the link at node {} points to a value that is not written before it",
                id.index()
            ))
        })?;
        self.out.push(b'@');
        self.long(long_of(index)?)
    }

    /// The `:E` instance variable of a string, regexp or `_dump` value (nothing for binary).
    fn encoding_of(&mut self, enc: Enc, depth: usize) -> Result<()> {
        match enc {
            Enc::Binary => Ok(()),
            Enc::Utf8 => self.encoding_ivar(true, depth),
            Enc::Ascii => self.encoding_ivar(false, depth),
        }
    }

    fn hash(&mut self, id: NodeId, with_default: bool, depth: usize) -> Result<()> {
        self.register(id)?;
        self.out.push(if with_default { b'}' } else { b'{' });
        match &self.node(id)?.value {
            Value::Object(entries) => {
                let (pairs, default) = split_default(entries, with_default, id)?;
                self.long(long_of(pairs.len())?)?;
                for (name, child) in pairs {
                    self.symbol(name, depth + 1)?;
                    self.value(*child, depth + 1, None)?;
                }
                if let Some((_, value)) = default {
                    self.value(*value, depth + 1, None)?;
                }
            }
            Value::Map(entries) => {
                let (pairs, default) = split_default(entries, with_default, id)?;
                self.long(long_of(pairs.len())?)?;
                for (key, child) in pairs {
                    self.value(*key, depth + 1, None)?;
                    self.value(*child, depth + 1, None)?;
                }
                if let Some((key, value)) = default {
                    if self.kind(*key)? != Kind::DefaultKey {
                        return Err(write_error(format!(
                            "the hash at node {} lost its default entry",
                            id.index()
                        )));
                    }
                    self.value(*value, depth + 1, None)?;
                }
            }
            _ => return Err(write_error("a hash changed its type")),
        }
        Ok(())
    }
}

fn split_default<T>(entries: &[T], with_default: bool, id: NodeId) -> Result<(&[T], Option<&T>)> {
    if !with_default {
        return Ok((entries, None));
    }
    match entries.split_last() {
        Some((default, pairs)) => Ok((pairs, Some(default))),
        None => Err(write_error(format!(
            "the hash at node {} lost its default entry",
            id.index()
        ))),
    }
}

fn changed(id: NodeId) -> Error {
    write_error(format!(
        "the value at node {} changed its type; a Ruby value keeps its type",
        id.index()
    ))
}

fn text_bytes(value: &Value) -> Option<&[u8]> {
    match value {
        Value::Str(text) => Some(text.as_bytes()),
        Value::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}
