//! Event-driven YAML parsing into a document with byte spans.

use saphyr_parser::{Event, Parser, ScalarStyle, Span, SpannedEventReceiver};
use saveprism_core::{
    DecodeCtx, Error, FloatWidth, IntWidth, NodeId, Result, SaveDoc, Value, codes,
};

use super::{PREFIX, YAML_PARSE};
use crate::text_common::{append_numbered, node, read_only};

struct Collector<'a> {
    events: Vec<(Event<'a>, Span)>,
}

impl<'a> SpannedEventReceiver<'a> for Collector<'a> {
    fn on_event(&mut self, event: Event<'a>, span: Span) {
        self.events.push((event, span));
    }
}

/// True when `text` parses as a YAML stream.
pub(crate) fn well_formed(text: &str) -> bool {
    let mut collector = Collector { events: Vec::new() };
    let mut parser = Parser::new_from_str(text);
    parser.load(&mut collector, true).is_ok()
}

/// saphyr's marker index counts characters for `Parser::new_from_str`; convert it to a byte
/// offset so the writer's spans line up with the UTF-8 text.
fn byte_offset(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

struct MapFrame {
    entries: Vec<(String, NodeId)>,
    pending: Option<(String, NodeId)>,
}

enum Frame {
    Map(MapFrame),
    Seq(Vec<NodeId>),
}

struct Builder<'d> {
    doc: &'d mut SaveDoc,
    stack: Vec<Frame>,
    documents: Vec<NodeId>,
    document_root: Option<NodeId>,
    base: usize,
}

/// Parses YAML text into a root array of documents.
pub(crate) fn parse(text: &str, base: usize, ctx: &mut DecodeCtx<'_>) -> Result<SaveDoc> {
    let mut collector = Collector { events: Vec::new() };
    let mut parser = Parser::new_from_str(text);
    parser
        .load(&mut collector, true)
        .map_err(|error| Error::new(YAML_PARSE, format!("invalid YAML: {error}")))?;
    let max_depth = ctx.limits().max_depth;
    let mut doc = ctx.new_doc();
    let mut builder = Builder {
        doc: &mut doc,
        stack: Vec::new(),
        documents: Vec::new(),
        document_root: None,
        base,
    };
    for (event, span) in collector.events {
        builder.event(event, span, text, max_depth)?;
    }
    let mut documents = builder.documents;
    if builder.document_root.is_some() {
        return Err(Error::new(YAML_PARSE, "unterminated YAML document"));
    }
    if documents.is_empty() {
        let empty = builder
            .doc
            .push(read_only(Value::Null, &format!("{PREFIX}null")))?;
        documents.push(empty);
    }
    let root = builder.doc.push(read_only(
        Value::Array(documents),
        &format!("{PREFIX}documents"),
    ))?;
    builder.doc.set_root(root);
    Ok(doc)
}

impl Builder<'_> {
    fn event(&mut self, event: Event<'_>, span: Span, text: &str, max_depth: usize) -> Result<()> {
        match event {
            Event::StreamStart | Event::StreamEnd | Event::Nothing => Ok(()),
            Event::DocumentStart(_) => {
                if self.document_root.is_some() {
                    return Err(Error::new(YAML_PARSE, "nested YAML document start"));
                }
                self.document_root = None;
                Ok(())
            }
            Event::DocumentEnd => {
                let root = match self.document_root.take() {
                    Some(id) => id,
                    None => self
                        .doc
                        .push(read_only(Value::Null, &format!("{PREFIX}null")))?,
                };
                self.documents.push(root);
                Ok(())
            }
            Event::Scalar(value, style, anchor, tag) => {
                let locked = anchor != 0 || tag.is_some();
                let start = self
                    .base
                    .saturating_add(byte_offset(text, span.start.index()));
                let end = self
                    .base
                    .saturating_add(byte_offset(text, span.end.index()));
                let (parsed, hint) = scalar_value(&value, style);
                let editable = !locked
                    && !matches!(style, ScalarStyle::Folded)
                    && !matches!(parsed, Value::Null)
                    && end >= start;
                let style_name = style_name(style);
                let leaf = node(
                    parsed,
                    &format!("{PREFIX}{style_name}:{hint}"),
                    Some((start, end)),
                    editable,
                );
                let id = self.doc.push(leaf)?;
                self.attach(id)
            }
            Event::Alias(anchor) => {
                let start = self
                    .base
                    .saturating_add(byte_offset(text, span.start.index()));
                let end = self
                    .base
                    .saturating_add(byte_offset(text, span.end.index()));
                let id = self.doc.push(node(
                    Value::Str(format!("*{anchor}")),
                    &format!("{PREFIX}alias"),
                    Some((start, end)),
                    false,
                ))?;
                self.attach(id)
            }
            Event::MappingStart(anchor, tag) => {
                if self.stack.len() >= max_depth {
                    return Err(Error::new(
                        codes::CORE_LIMIT_DEPTH,
                        format!("YAML nesting limit of {max_depth} exceeded"),
                    ));
                }
                let _ = (anchor, tag);
                self.stack.push(Frame::Map(MapFrame {
                    entries: Vec::new(),
                    pending: None,
                }));
                Ok(())
            }
            Event::SequenceStart(anchor, tag) => {
                if self.stack.len() >= max_depth {
                    return Err(Error::new(
                        codes::CORE_LIMIT_DEPTH,
                        format!("YAML nesting limit of {max_depth} exceeded"),
                    ));
                }
                let _ = (anchor, tag);
                self.stack.push(Frame::Seq(Vec::new()));
                Ok(())
            }
            Event::MappingEnd | Event::SequenceEnd => {
                let Some(frame) = self.stack.pop() else {
                    return Err(Error::new(YAML_PARSE, "unbalanced YAML collection"));
                };
                let id = match frame {
                    Frame::Map(frame) => {
                        if frame.pending.is_some() {
                            return Err(Error::new(
                                YAML_PARSE,
                                "YAML mapping ends with a key but no value",
                            ));
                        }
                        self.doc.push(read_only(
                            Value::Object(frame.entries),
                            &format!("{PREFIX}mapping"),
                        ))?
                    }
                    Frame::Seq(items) => self
                        .doc
                        .push(read_only(Value::Array(items), &format!("{PREFIX}sequence")))?,
                };
                self.attach(id)
            }
        }?;
        Ok(())
    }

    fn attach(&mut self, id: NodeId) -> Result<()> {
        let key = self.key_text(id);
        match self.stack.last_mut() {
            Some(Frame::Seq(items)) => {
                items.push(id);
                Ok(())
            }
            Some(Frame::Map(frame)) => {
                if frame.pending.is_none() {
                    frame.pending = Some((key, id));
                    Ok(())
                } else if let Some((key, _)) = frame.pending.take() {
                    append_numbered(
                        self.doc,
                        &mut frame.entries,
                        &format!("{PREFIX}keys"),
                        key,
                        id,
                    )
                } else {
                    Ok(())
                }
            }
            None => {
                if self.document_root.is_some() {
                    return Err(Error::new(
                        YAML_PARSE,
                        "a YAML document has more than one root node",
                    ));
                }
                self.document_root = Some(id);
                Ok(())
            }
        }
    }

    fn key_text(&self, id: NodeId) -> String {
        match self.doc.get(id).map(|node| &node.value) {
            Some(Value::Str(text)) => text.clone(),
            Some(Value::Int { v, .. }) => v.to_string(),
            Some(Value::Float { v, .. }) => v.to_string(),
            Some(Value::Bool(value)) => value.to_string(),
            Some(Value::Null) => "null".to_owned(),
            Some(Value::Array(_)) => "#key-sequence".to_owned(),
            Some(Value::Map(_)) | Some(Value::Object(_)) => "#key-mapping".to_owned(),
            Some(Value::Bytes(_)) => "#key-binary".to_owned(),
            Some(Value::Ref(_)) => "#key-ref".to_owned(),
            None => "#key".to_owned(),
        }
    }
}

fn scalar_value(value: &str, style: ScalarStyle) -> (Value, &'static str) {
    if matches!(style, ScalarStyle::Plain) {
        if matches!(value, "" | "~" | "null" | "Null" | "NULL") {
            return (Value::Null, "null");
        }
        if matches!(value, "true" | "True" | "TRUE") {
            return (Value::Bool(true), "bool");
        }
        if matches!(value, "false" | "False" | "FALSE") {
            return (Value::Bool(false), "bool");
        }
        if let Some(number) = parse_int(value) {
            return (
                Value::Int {
                    v: number,
                    width: IntWidth::Unbounded,
                },
                "int",
            );
        }
        if let Some(number) = parse_float(value) {
            return (
                Value::Float {
                    v: number,
                    width: FloatWidth::F64,
                },
                "float",
            );
        }
    }
    (Value::Str(value.to_owned()), "string")
}

fn style_name(style: ScalarStyle) -> &'static str {
    match style {
        ScalarStyle::Plain => "plain",
        ScalarStyle::SingleQuoted => "single",
        ScalarStyle::DoubleQuoted => "double",
        ScalarStyle::Literal => "literal",
        ScalarStyle::Folded => "folded",
    }
}

fn parse_int(text: &str) -> Option<i128> {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    let (sign, unsigned) = match clean.strip_prefix('-') {
        Some(rest) => (-1i128, rest),
        None => (1i128, clean.strip_prefix('+').unwrap_or(clean.as_str())),
    };
    if let Some(hex) = unsigned.strip_prefix("0x") {
        return i128::from_str_radix(hex, 16).ok()?.checked_mul(sign);
    }
    if let Some(octal) = unsigned.strip_prefix("0o") {
        return i128::from_str_radix(octal, 8).ok()?.checked_mul(sign);
    }
    if unsigned.is_empty() || !unsigned.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    unsigned.parse::<i128>().ok()?.checked_mul(sign)
}

fn parse_float(text: &str) -> Option<f64> {
    let clean: String = text.chars().filter(|c| *c != '_').collect();
    if matches!(clean.as_str(), ".inf" | "+.inf" | ".Inf" | "+.Inf") {
        return Some(f64::INFINITY);
    }
    if matches!(clean.as_str(), "-.inf" | "-.Inf") {
        return Some(f64::NEG_INFINITY);
    }
    if matches!(clean.as_str(), ".nan" | ".NaN" | ".NAN") {
        return Some(f64::NAN);
    }
    if !clean.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
        return None;
    }
    clean.parse::<f64>().ok()
}
