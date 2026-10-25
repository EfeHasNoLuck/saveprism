//! XML property lists (`<plist version="1.0">`), parsed with the plist types.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use saveprism_core::{
    DecodeCtx, Error, FloatWidth, IntWidth, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint,
    Value, codes,
};

use super::{PLIST_PARSE, PREFIX};
use crate::text_common::{escape_xml, format_float, node, read_only};

enum Token {
    Start {
        name: String,
        start: usize,
        end: usize,
    },
    Empty {
        name: String,
        start: usize,
        end: usize,
    },
    End {
        name: String,
        start: usize,
        end: usize,
    },
    Text {
        raw: String,
        start: usize,
        end: usize,
    },
    Other,
}

/// Parses an XML plist; the root node is the single value inside `<plist>`.
pub(crate) fn parse(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<SaveDoc> {
    let (body, base) = match input.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (rest, 3usize),
        None => (input, 0usize),
    };
    let text = std::str::from_utf8(body)
        .map_err(|_| Error::new(PLIST_PARSE, "XML plist is not valid UTF-8"))?;
    let mut reader = Reader::from_str(text);
    let mut tokens = Vec::new();
    loop {
        let start =
            base.saturating_add(usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX));
        let event = reader
            .read_event()
            .map_err(|error| Error::new(PLIST_PARSE, format!("plist token error: {error}")))?;
        let end =
            base.saturating_add(usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX));
        if end > input.len() || start > end {
            return Err(Error::new(PLIST_PARSE, "invalid plist token position"));
        }
        let token = match event {
            Event::Start(element) => Token::Start {
                name: name(&element)?,
                start,
                end,
            },
            Event::Empty(element) => Token::Empty {
                name: name(&element)?,
                start,
                end,
            },
            Event::End(element) => Token::End {
                name: element.name().as_ref().to_owned(),
                start,
                end,
            },
            Event::Text(data) => Token::Text {
                raw: data.into_inner().into_owned(),
                start,
                end,
            },
            Event::CData(data) => Token::Text {
                raw: data.into_inner().into_owned(),
                start,
                end,
            },
            Event::GeneralRef(_) => Token::Text {
                raw: String::from_utf8_lossy(
                    body.get(start.saturating_sub(base)..end.saturating_sub(base))
                        .unwrap_or_default(),
                )
                .into_owned(),
                start,
                end,
            },
            Event::Eof => break,
            _ => Token::Other,
        };
        tokens.push(token);
    }
    let mut doc = ctx.new_doc();
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
        doc: &mut doc,
        max_depth: ctx.limits().max_depth,
    };
    let id = parser.document()?;
    parser.doc.set_root(id);
    Ok(doc)
}

struct Parser<'a, 'd> {
    tokens: &'a [Token],
    at: usize,
    doc: &'d mut SaveDoc,
    max_depth: usize,
}

impl Parser<'_, '_> {
    fn document(&mut self) -> Result<NodeId> {
        self.skip_trivia();
        let mut saw_plist = false;
        let mut value = None;
        while let Some(token) = self.tokens.get(self.at) {
            match token {
                Token::Start { name, .. } if name == "plist" => {
                    saw_plist = true;
                    self.at += 1;
                }
                Token::End { name, .. } if name == "plist" => {
                    self.at += 1;
                    break;
                }
                Token::Text { raw, .. } if raw.trim().is_empty() => {
                    self.at += 1;
                }
                Token::Other => {
                    self.at += 1;
                }
                _ if value.is_none() => {
                    value = Some(self.value(0)?);
                }
                _ => {
                    return Err(Error::new(PLIST_PARSE, "more than one value in the plist"));
                }
            }
        }
        if !saw_plist {
            return Err(Error::new(PLIST_PARSE, "the plist element is missing"));
        }
        value.ok_or_else(|| Error::new(PLIST_PARSE, "the plist has no value"))
    }

    fn value(&mut self, depth: usize) -> Result<NodeId> {
        if depth > self.max_depth {
            return Err(Error::new(
                codes::CORE_LIMIT_DEPTH,
                format!("plist nesting limit of {} exceeded", self.max_depth),
            ));
        }
        self.skip_trivia();
        let Some(token) = self.tokens.get(self.at) else {
            return Err(Error::new(PLIST_PARSE, "the plist ends before a value"));
        };
        match token {
            Token::Start { name, start, end } => {
                let tag = name.clone();
                let tag_start = *start;
                let tag_end = *end;
                self.at += 1;
                match tag.as_str() {
                    "dict" => self.dict(depth),
                    "array" => self.array(depth),
                    "true" | "false" => {
                        let value = tag == "true";
                        let closing = self.consume_end(&tag)?;
                        let span = (tag_start, closing.max(tag_end));
                        self.doc.push(node(
                            Value::Bool(value),
                            &format!("{PREFIX}bool"),
                            Some(span),
                            true,
                        ))
                    }
                    "string" => {
                        let (raw, span) = self.content("string")?;
                        let text = unescape(&raw);
                        self.doc.push(node(
                            Value::Str(text),
                            &format!("{PREFIX}string"),
                            Some(span),
                            true,
                        ))
                    }
                    "integer" => {
                        let (raw, span) = self.content("integer")?;
                        let value = raw
                            .trim()
                            .parse::<i128>()
                            .map_err(|_| Error::new(PLIST_PARSE, "invalid plist integer"))?;
                        self.doc.push(node(
                            Value::Int {
                                v: value,
                                width: IntWidth::Unbounded,
                            },
                            &format!("{PREFIX}int"),
                            Some(span),
                            true,
                        ))
                    }
                    "real" => {
                        let (raw, span) = self.content("real")?;
                        let value = raw
                            .trim()
                            .parse::<f64>()
                            .map_err(|_| Error::new(PLIST_PARSE, "invalid plist real"))?;
                        self.doc.push(node(
                            Value::Float {
                                v: value,
                                width: FloatWidth::F64,
                            },
                            &format!("{PREFIX}float"),
                            Some(span),
                            true,
                        ))
                    }
                    "date" => {
                        let (raw, span) = self.content("date")?;
                        self.doc.push(node(
                            Value::Str(raw),
                            &format!("{PREFIX}date"),
                            Some(span),
                            false,
                        ))
                    }
                    "data" => {
                        let (raw, span) = self.content("data")?;
                        self.doc.push(node(
                            Value::Str(raw),
                            &format!("{PREFIX}data"),
                            Some(span),
                            false,
                        ))
                    }
                    other => Err(Error::new(
                        PLIST_PARSE,
                        format!("unknown plist element <{other}>"),
                    )),
                }
            }
            Token::Empty { name, start, end } => {
                let tag = name.clone();
                let span = (*start, *end);
                self.at += 1;
                match tag.as_str() {
                    "true" | "false" => self.doc.push(node(
                        Value::Bool(tag == "true"),
                        &format!("{PREFIX}bool"),
                        Some(span),
                        true,
                    )),
                    "string" => self.doc.push(node(
                        Value::Str(String::new()),
                        &format!("{PREFIX}string"),
                        Some((span.1, span.1)),
                        true,
                    )),
                    "dict" => self.doc.push(read_only(
                        Value::Object(Vec::new()),
                        &format!("{PREFIX}dict"),
                    )),
                    "array" => self.doc.push(read_only(
                        Value::Array(Vec::new()),
                        &format!("{PREFIX}array"),
                    )),
                    "data" => self.doc.push(read_only(
                        Value::Str(String::new()),
                        &format!("{PREFIX}data"),
                    )),
                    other => Err(Error::new(
                        PLIST_PARSE,
                        format!("unknown empty plist element <{other}/>"),
                    )),
                }
            }
            _ => Err(Error::new(PLIST_PARSE, "expected a plist value element")),
        }
    }

    fn dict(&mut self, depth: usize) -> Result<NodeId> {
        let mut entries = Vec::new();
        loop {
            self.skip_trivia();
            match self.tokens.get(self.at) {
                Some(Token::End { name, .. }) if name == "dict" => {
                    self.at += 1;
                    break;
                }
                Some(Token::Start { name, .. }) if name == "key" => {
                    self.at += 1;
                    let (raw, _) = self.content("key")?;
                    let key = unescape(&raw);
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                }
                Some(_) => {
                    return Err(Error::new(PLIST_PARSE, "a plist dict expects <key>"));
                }
                None => return Err(Error::new(PLIST_PARSE, "unterminated plist dict")),
            }
        }
        let mut dict = Node::new(Value::Object(entries));
        dict.type_hint = Some(TypeHint::new(format!("{PREFIX}dict")));
        dict.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(dict)
    }

    fn array(&mut self, depth: usize) -> Result<NodeId> {
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            match self.tokens.get(self.at) {
                Some(Token::End { name, .. }) if name == "array" => {
                    self.at += 1;
                    break;
                }
                Some(Token::Text { raw, .. }) if raw.trim().is_empty() => {
                    self.at += 1;
                }
                Some(_) => items.push(self.value(depth + 1)?),
                None => return Err(Error::new(PLIST_PARSE, "unterminated plist array")),
            }
        }
        let mut array = Node::new(Value::Array(items));
        array.type_hint = Some(TypeHint::new(format!("{PREFIX}array")));
        array.flags.insert(NodeFlags::READ_ONLY);
        self.doc.push(array)
    }

    fn content(&mut self, closing: &str) -> Result<(String, (usize, usize))> {
        let mut raw = String::new();
        let mut start = None;
        let mut end = 0usize;
        loop {
            match self.tokens.get(self.at) {
                Some(Token::Text {
                    raw: chunk,
                    start: chunk_start,
                    end: chunk_end,
                }) => {
                    if start.is_none() {
                        start = Some(*chunk_start);
                    }
                    raw.push_str(chunk);
                    end = *chunk_end;
                    self.at += 1;
                }
                Some(Token::End {
                    name,
                    start: closing_start,
                    end: closing_end,
                }) if name == closing => {
                    let start = start.unwrap_or(*closing_start);
                    let end = if end >= start { end } else { *closing_start };
                    let _ = closing_end;
                    self.at += 1;
                    return Ok((raw, (start, end.max(start))));
                }
                Some(Token::Other) => {
                    self.at += 1;
                }
                _ => {
                    return Err(Error::new(
                        PLIST_PARSE,
                        format!("unterminated plist <{closing}>"),
                    ));
                }
            }
        }
    }

    fn consume_end(&mut self, tag: &str) -> Result<usize> {
        loop {
            match self.tokens.get(self.at) {
                Some(Token::End { name, end, .. }) if name == tag => {
                    let end = *end;
                    self.at += 1;
                    return Ok(end);
                }
                Some(_) => self.at += 1,
                None => {
                    return Err(Error::new(
                        PLIST_PARSE,
                        format!("unterminated plist <{tag}>"),
                    ));
                }
            }
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.tokens.get(self.at) {
                Some(Token::Other) => self.at += 1,
                Some(Token::Text { raw, .. }) if raw.trim().is_empty() => self.at += 1,
                _ => break,
            }
        }
    }
}

fn name(element: &quick_xml::events::BytesStart<'_>) -> Result<String> {
    Ok(element.name().as_ref().to_owned())
}

fn unescape(raw: &str) -> String {
    match quick_xml::escape::unescape(raw) {
        Ok(value) => value.into_owned(),
        Err(_) => raw.to_owned(),
    }
}

/// Renders an edited XML plist leaf.
pub(crate) fn render(hint: &str, value: &Value) -> Option<String> {
    match (hint, value) {
        ("PLIST:string", Value::Str(text)) => Some(escape_xml(text)),
        ("PLIST:int", Value::Int { v, .. }) => Some(v.to_string()),
        ("PLIST:float", Value::Float { v, .. }) => Some(format_float(*v)),
        ("PLIST:bool", Value::Bool(true)) => Some("<true/>".to_owned()),
        ("PLIST:bool", Value::Bool(false)) => Some("<false/>".to_owned()),
        _ => None,
    }
}
