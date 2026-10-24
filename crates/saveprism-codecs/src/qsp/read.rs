//! QSP reader: bytes to the data tree (see `mod.rs` for the layout and the tree).

use saveprism_core::{
    DecodeCtx, Error, IntWidth, Node, NodeFlags, NodeId, ParamValue, Params, Result, SaveDoc,
    TypeHint, Value,
};

use super::{
    ANSI_MAGIC, Encoding, MAX_NESTING, P_ENCODING, P_VARIANT_TYPES, QSP_PARSE, QSP_UNSUPPORTED,
    SHIFT, SIGNATURE, UTF16_MAGIC, ansi_to_char,
};

/// One line of the save; the unit width follows the file encoding.
#[derive(Clone, Debug)]
enum Line {
    Wide(Vec<u16>),
    Byte(Vec<u8>),
}

/// Reads the whole file: signature, version, then the fields in QSP's order.
pub(super) fn read_all(input: &[u8], ctx: &DecodeCtx<'_>) -> Result<(SaveDoc, Params)> {
    let encoding = if input.starts_with(&UTF16_MAGIC) {
        Encoding::Utf16Le
    } else if input.starts_with(ANSI_MAGIC) {
        Encoding::Ansi
    } else {
        return Err(parse("not a QSP save (no QSPSAVEDGAME signature)"));
    };
    let lines = split_lines(input, encoding)?;
    let mut reader = Reader {
        lines,
        pos: 0,
        doc: ctx.new_doc(),
        params: Params::new(),
        variant_types: Params::new(),
        variant_ordinal: 0,
    };
    reader.params.set(P_ENCODING, encoding.name());
    reader.save()
}

fn parse(message: impl Into<String>) -> Error {
    Error::new(QSP_PARSE, message)
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(QSP_UNSUPPORTED, message)
}

/// Splits the input at CRLF. The file must end with a separator (QSP always writes a final empty
/// line); a lone CR or LF is part of the line, as in QSP's own splitter.
fn split_lines(input: &[u8], encoding: Encoding) -> Result<Vec<Line>> {
    match encoding {
        Encoding::Utf16Le => {
            if !input.len().is_multiple_of(2) {
                return Err(parse("a UTF-16 save has an odd number of bytes"));
            }
            let (pairs, _rest) = input.as_chunks::<2>();
            let mut units = Vec::with_capacity(pairs.len());
            for pair in pairs {
                units.push(u16::from_le_bytes(*pair));
            }
            let mut lines = Vec::new();
            let mut current = Vec::new();
            let mut index = 0;
            while let Some(unit) = units.get(index).copied() {
                if unit == 0x000D && units.get(index + 1).copied() == Some(0x000A) {
                    lines.push(Line::Wide(std::mem::take(&mut current)));
                    index += 2;
                } else {
                    current.push(unit);
                    index += 1;
                }
            }
            if !current.is_empty() {
                return Err(parse("the save does not end with a CRLF separator"));
            }
            Ok(lines)
        }
        Encoding::Ansi => {
            let mut lines = Vec::new();
            let mut current = Vec::new();
            let mut index = 0;
            while let Some(byte) = input.get(index).copied() {
                if byte == 0x0D && input.get(index + 1).copied() == Some(0x0A) {
                    lines.push(Line::Byte(std::mem::take(&mut current)));
                    index += 2;
                } else {
                    current.push(byte);
                    index += 1;
                }
            }
            if !current.is_empty() {
                return Err(parse("the save does not end with a CRLF separator"));
            }
            Ok(lines)
        }
    }
}

/// One decoded code unit (UTF-16) after undoing QSP's character shift.
fn decode_unit(unit: u16) -> u16 {
    if unit == 0u16.wrapping_sub(SHIFT) {
        5
    } else if unit == 0 {
        0
    } else {
        unit.wrapping_add(SHIFT)
    }
}

/// One decoded byte (single-byte text) after undoing QSP's character shift.
fn decode_byte(byte: u8) -> u8 {
    let special = 0u8.wrapping_sub(u8::try_from(SHIFT).unwrap_or(5));
    if byte == special {
        5
    } else if byte == 0 {
        0
    } else {
        byte.wrapping_add(u8::try_from(SHIFT).unwrap_or(5))
    }
}

/// The plain text of a raw line (the signature and the version); must be ASCII.
fn raw_text(line: &Line) -> Result<String> {
    match line {
        Line::Wide(units) => {
            let mut text = String::with_capacity(units.len());
            for unit in units {
                let ch = char::from_u32(u32::from(*unit))
                    .filter(|ch| ch.is_ascii())
                    .ok_or_else(|| parse("the save header is not ASCII"))?;
                text.push(ch);
            }
            Ok(text)
        }
        Line::Byte(bytes) => {
            let mut text = String::with_capacity(bytes.len());
            for byte in bytes {
                let ch = char::from(*byte);
                if !ch.is_ascii() {
                    return Err(parse("the save header is not ASCII"));
                }
                text.push(ch);
            }
            Ok(text)
        }
    }
}

/// The plain text of an encoded line.
fn encoded_text(line: &Line) -> Result<String> {
    match line {
        Line::Wide(units) => {
            let decoded: Vec<u16> = units.iter().copied().map(decode_unit).collect();
            String::from_utf16(&decoded).map_err(|_| parse("a text field is not valid UTF-16"))
        }
        Line::Byte(bytes) => {
            let mut text = String::with_capacity(bytes.len());
            for byte in bytes {
                text.push(ansi_to_char(decode_byte(*byte)));
            }
            Ok(text)
        }
    }
}

/// Splits `text` into the integer QSP reads with `qspStrToNum`, plus the original text when the
/// canonical decimal would be different.
fn parse_number(text: &str) -> Result<(i64, Option<String>)> {
    let trimmed = text.trim_matches(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'));
    if trimmed.is_empty() {
        return Ok((0, Some(text.to_string())));
    }
    let (negative, digits) = match trimmed.as_bytes().first().copied() {
        Some(b'-') => (true, trimmed.get(1..).unwrap_or_default()),
        Some(b'+') => (false, trimmed.get(1..).unwrap_or_default()),
        _ => (false, trimmed),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(parse(format!("'{text}' is not a whole number")));
    }
    let mut value: i128 = 0;
    for byte in digits.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(i128::from(byte - b'0')))
            .ok_or_else(|| parse(format!("the number '{text}' is too large")))?;
    }
    let value = if negative { -value } else { value };
    let value = i64::try_from(value)
        .map_err(|_| parse(format!("the number '{text}' does not fit 64 bits")))?;
    let canonical = value.to_string();
    let lexeme = (text != canonical).then(|| text.to_string());
    Ok((value, lexeme))
}

/// The kind of one value, from its encoded type code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VariantKind {
    Undef,
    Tuple,
    Num,
    Bool,
    Str,
    Code,
    VarRef,
}

impl VariantKind {
    pub(super) fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(VariantKind::Undef),
            1 => Some(VariantKind::Tuple),
            2 => Some(VariantKind::Num),
            3 => Some(VariantKind::Bool),
            4 => Some(VariantKind::Str),
            5 => Some(VariantKind::Code),
            6 => Some(VariantKind::VarRef),
            _ => None,
        }
    }

    pub(super) fn code(self) -> i64 {
        match self {
            VariantKind::Undef => 0,
            VariantKind::Tuple => 1,
            VariantKind::Num => 2,
            VariantKind::Bool => 3,
            VariantKind::Str => 4,
            VariantKind::Code => 5,
            VariantKind::VarRef => 6,
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            VariantKind::Undef => "QSP:undef",
            VariantKind::Tuple => "QSP:tuple",
            VariantKind::Num => "QSP:num",
            VariantKind::Bool => "QSP:bool",
            VariantKind::Str => "QSP:str",
            VariantKind::Code => "QSP:code",
            VariantKind::VarRef => "QSP:varref",
        }
    }
}

/// What `next_line` produced: the line and its plain text.
struct Field {
    text: String,
}

struct Reader {
    lines: Vec<Line>,
    pos: usize,
    doc: SaveDoc,
    params: Params,
    variant_types: Params,
    variant_ordinal: u64,
}

impl Reader {
    fn remaining(&self) -> usize {
        self.lines.len().saturating_sub(self.pos)
    }

    fn next_line(&mut self) -> Result<&Line> {
        let line = self
            .lines
            .get(self.pos)
            .ok_or_else(|| parse("the save ends before its last field"))?;
        self.pos += 1;
        Ok(line)
    }

    fn raw(&mut self) -> Result<String> {
        let line = self.next_line()?.clone();
        raw_text(&line)
    }

    fn field(&mut self) -> Result<Field> {
        let line = self.next_line()?.clone();
        let text = encoded_text(&line)?;
        Ok(Field { text })
    }

    fn text(&mut self) -> Result<String> {
        Ok(self.field()?.text)
    }

    fn number(&mut self) -> Result<(i64, Option<String>)> {
        let field = self.field()?;
        parse_number(&field.text)
    }

    fn count(&mut self, what: &str) -> Result<usize> {
        let (value, _) = self.number()?;
        if value < 0 {
            return Err(parse(format!("{what} is negative ({value})")));
        }
        let value = usize::try_from(value)
            .map_err(|_| parse(format!("{what} does not fit this platform")))?;
        if value > self.remaining() {
            return Err(parse(format!(
                "{what} is {value}, but only {} lines remain",
                self.remaining()
            )));
        }
        Ok(value)
    }

    fn push(&mut self, value: Value, hint: &str, read_only: bool) -> Result<NodeId> {
        let mut node = Node::new(value);
        node.type_hint = Some(TypeHint::new(hint));
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn push_text(&mut self, text: String, hint: &str, read_only: bool) -> Result<NodeId> {
        self.push(Value::Str(text), hint, read_only)
    }

    fn push_int(
        &mut self,
        value: i64,
        lexeme: Option<String>,
        hint: &str,
        read_only: bool,
    ) -> Result<NodeId> {
        let mut node = Node::new(Value::Int {
            v: i128::from(value),
            width: IntWidth::I64,
        });
        node.type_hint = Some(TypeHint::new(hint));
        node.lexeme = lexeme.map(Into::into);
        if read_only {
            node.flags.insert(NodeFlags::READ_ONLY);
        }
        self.doc.push(node)
    }

    fn push_object(&mut self, entries: Vec<(String, NodeId)>, hint: &str) -> Result<NodeId> {
        self.push(Value::Object(entries), hint, true)
    }

    fn push_array(&mut self, items: Vec<NodeId>, hint: &str) -> Result<NodeId> {
        self.push(Value::Array(items), hint, true)
    }

    /// Reads the whole save after the signature and version lines.
    fn save(mut self) -> Result<(SaveDoc, Params)> {
        let header = self.raw()?;
        if header != SIGNATURE {
            return Err(parse(format!("unknown save signature '{header}'")));
        }
        let version = self.raw()?;

        let mut entries: Vec<(String, NodeId)> = Vec::new();

        let header_id = self.push_text(header, "QSP:raw", true)?;
        entries.push(("header".to_string(), header_id));
        let version_id = self.push_text(version, "QSP:raw", true)?;
        entries.push(("version".to_string(), version_id));

        entries.push(("checksum".to_string(), self.int_field("QSP:int")?));
        entries.push(("playtime_ms".to_string(), self.int_field("QSP:int")?));
        entries.push(("selected_action".to_string(), self.int_field("QSP:int")?));
        entries.push(("selected_object".to_string(), self.int_field("QSP:int")?));
        entries.push(("view_path".to_string(), self.text_field("QSP:str")?));
        entries.push(("input".to_string(), self.text_field("QSP:str")?));
        entries.push(("description".to_string(), self.text_field("QSP:str")?));
        entries.push(("vars_description".to_string(), self.text_field("QSP:str")?));
        entries.push(("location".to_string(), self.text_field("QSP:str")?));
        entries.push(("windows_state".to_string(), self.int_field("QSP:int")?));
        entries.push(("timer_interval".to_string(), self.int_field("QSP:int")?));

        entries.push(("playlist".to_string(), self.string_list("QSP:str")?));
        entries.push(("includes".to_string(), self.string_list("QSP:str")?));
        entries.push(("actions".to_string(), self.actions()?));
        entries.push(("objects".to_string(), self.objects()?));
        entries.push(("object_groups".to_string(), self.object_groups()?));
        entries.push(("variables".to_string(), self.variables()?));

        if self.remaining() != 0 {
            return Err(parse(format!(
                "{} unexpected line(s) after the last variable",
                self.remaining()
            )));
        }

        let root = self.push_object(entries, "QSP:save")?;
        self.doc.set_root(root);
        if !self.variant_types.is_empty() {
            self.params
                .set(P_VARIANT_TYPES, ParamValue::Table(self.variant_types));
        }
        Ok((self.doc, self.params))
    }

    fn int_field(&mut self, hint: &str) -> Result<NodeId> {
        let (value, lexeme) = self.number()?;
        self.push_int(value, lexeme, hint, false)
    }

    fn text_field(&mut self, hint: &str) -> Result<NodeId> {
        let text = self.text()?;
        self.push_text(text, hint, false)
    }

    fn string_list(&mut self, hint: &str) -> Result<NodeId> {
        let count = self.count("a string list count")?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let text = self.text()?;
            items.push(self.push_text(text, hint, false)?);
        }
        self.push_array(items, "QSP:strings")
    }

    fn actions(&mut self) -> Result<NodeId> {
        let count = self.count("the action count")?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let mut action: Vec<(String, NodeId)> = Vec::new();
            action.push(("description".to_string(), self.text_field("QSP:str")?));
            action.push(("image".to_string(), self.text_field("QSP:str")?));
            let line_count = self.count("an action line count")?;
            let mut lines = Vec::with_capacity(line_count);
            for _ in 0..line_count {
                let text = self.text()?;
                let text_id = self.push_text(text, "QSP:str", false)?;
                let number = self.int_field("QSP:int")?;
                let line: Vec<(String, NodeId)> = vec![
                    ("text".to_string(), text_id),
                    ("line_number".to_string(), number),
                ];
                lines.push(self.push_object(line, "QSP:line")?);
            }
            let lines_id = self.push_array(lines, "QSP:lines")?;
            action.push(("lines".to_string(), lines_id));
            action.push(("location".to_string(), self.text_field("QSP:str")?));
            action.push(("action_index".to_string(), self.int_field("QSP:int")?));
            items.push(self.push_object(action, "QSP:action")?);
        }
        self.push_array(items, "QSP:actions")
    }

    fn objects(&mut self) -> Result<NodeId> {
        let count = self.count("the object count")?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let object: Vec<(String, NodeId)> = vec![
                ("name".to_string(), self.text_field("QSP:str")?),
                ("image".to_string(), self.text_field("QSP:str")?),
            ];
            items.push(self.push_object(object, "QSP:object")?);
        }
        self.push_array(items, "QSP:objects")
    }

    fn object_groups(&mut self) -> Result<NodeId> {
        let count = self.count("the object group count")?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let group: Vec<(String, NodeId)> = vec![
                ("name".to_string(), self.text_field("QSP:str")?),
                ("description".to_string(), self.text_field("QSP:str")?),
                ("image".to_string(), self.text_field("QSP:str")?),
                ("updated_fields".to_string(), self.int_field("QSP:int")?),
                ("objects_count".to_string(), self.int_field("QSP:int")?),
            ];
            items.push(self.push_object(group, "QSP:object_group")?);
        }
        self.push_array(items, "QSP:object_groups")
    }

    fn variables(&mut self) -> Result<NodeId> {
        let count = self.count("the variable count")?;
        let mut items = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.text()?;
            let name_id = self.push_text(name, "QSP:str", true)?;
            let value_count = self.count("a variable value count")?;
            let mut values = Vec::with_capacity(value_count);
            for _ in 0..value_count {
                values.push(self.variant(1)?);
            }
            let values_id = self.push_array(values, "QSP:values")?;
            let key_count = self.count("a variable key count")?;
            if key_count > value_count {
                return Err(parse(format!(
                    "a variable has {key_count} keys for {value_count} values"
                )));
            }
            let mut keys = Vec::with_capacity(key_count);
            for _ in 0..key_count {
                let mut key: Vec<(String, NodeId)> = Vec::new();
                let (index, lexeme) = self.number()?;
                key.push((
                    "index".to_string(),
                    self.push_int(index, lexeme, "QSP:int", true)?,
                ));
                let value = self.variant(1)?;
                key.push(("key".to_string(), value));
                keys.push(self.push_object(key, "QSP:key")?);
            }
            let keys_id = self.push_array(keys, "QSP:keys")?;
            let variable: Vec<(String, NodeId)> = vec![
                ("name".to_string(), name_id),
                ("values".to_string(), values_id),
                ("keys".to_string(), keys_id),
            ];
            items.push(self.push_object(variable, "QSP:variable")?);
        }
        self.push_array(items, "QSP:variables")
    }

    /// One value: a type line, then the payload. Tuples nest.
    fn variant(&mut self, depth: usize) -> Result<NodeId> {
        if depth > MAX_NESTING {
            return Err(parse("values are nested too deeply"));
        }
        let (code, type_lexeme) = self.number()?;
        let kind = VariantKind::from_code(code)
            .ok_or_else(|| unsupported(format!("value type {code} is not supported")))?;
        if let Some(text) = type_lexeme {
            let key = self.variant_ordinal.to_string();
            self.variant_types.set(key, ParamValue::Str(text));
        }
        self.variant_ordinal = self.variant_ordinal.saturating_add(1);
        match kind {
            VariantKind::Undef => self.push(Value::Null, kind.hint(), true),
            VariantKind::Tuple => {
                let count = self.count("a tuple value count")?;
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.variant(depth + 1)?);
                }
                self.push_array(items, kind.hint())
            }
            VariantKind::Num | VariantKind::Bool => {
                let (value, lexeme) = self.number()?;
                self.push_int(value, lexeme, kind.hint(), false)
            }
            VariantKind::Str | VariantKind::Code | VariantKind::VarRef => {
                let text = self.text()?;
                self.push_text(text, kind.hint(), false)
            }
        }
    }
}

/// The code of a value kind, used by the writer.
pub(super) fn variant_kind_from_hint(hint: &str) -> Option<(i64, VariantKind)> {
    let kind = match hint.strip_prefix(super::HINT_PREFIX)? {
        "undef" => VariantKind::Undef,
        "tuple" => VariantKind::Tuple,
        "num" => VariantKind::Num,
        "bool" => VariantKind::Bool,
        "str" => VariantKind::Str,
        "code" => VariantKind::Code,
        "varref" => VariantKind::VarRef,
        _ => return None,
    };
    Some((kind.code(), kind))
}
