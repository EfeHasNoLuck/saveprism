//! Tokenizing XML into a [`SaveDoc`](saveprism_core::SaveDoc) with byte spans.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use saveprism_core::{DecodeCtx, Error, NodeId, Result, SaveDoc, Value, codes};

use super::{PREFIX, XML_PARSE};
use crate::text_common::{append_numbered, is_blank, node, read_only};

struct Frame {
    name: String,
    entries: Vec<(String, NodeId)>,
    text_start: Option<usize>,
    text_end: usize,
}

/// Parses XML `input`; the caller keeps the original bytes as the codec parameter.
pub(crate) fn parse(input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<SaveDoc> {
    let (body, base) = match input.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(rest) => (rest, 3usize),
        None => (input, 0usize),
    };
    let text =
        std::str::from_utf8(body).map_err(|_| Error::new(XML_PARSE, "XML is not valid UTF-8"))?;
    let mut doc = ctx.new_doc();
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Frame> = Vec::new();
    let mut root_entries: Vec<(String, NodeId)> = Vec::new();
    let mut root_element: Option<NodeId> = None;
    let max_depth = ctx.limits().max_depth;

    loop {
        let start = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        let event = reader
            .read_event()
            .map_err(|error| Error::new(XML_PARSE, format!("XML token error: {error}")))?;
        let end = usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
        if end > body.len() || start > end {
            return Err(Error::new(XML_PARSE, "invalid XML event position"));
        }
        match event {
            Event::Start(element) => {
                flush_text(&mut doc, stack.last_mut(), body, base)?;
                let name = name_of(element.name().as_ref())?;
                let frame = new_frame(&mut doc, &name, text, start, end, base)?;
                if stack.len() >= max_depth {
                    return Err(Error::new(
                        codes::CORE_LIMIT_DEPTH,
                        format!("XML nesting limit of {max_depth} exceeded"),
                    ));
                }
                stack.push(frame);
            }
            Event::Empty(element) => {
                flush_text(&mut doc, stack.last_mut(), body, base)?;
                let name = name_of(element.name().as_ref())?;
                let frame = new_frame(&mut doc, &name, text, start, end, base)?;
                let id = doc.push(node(
                    Value::Object(frame.entries),
                    &format!("{PREFIX}element"),
                    None,
                    false,
                ))?;
                attach(
                    &mut doc,
                    stack.last_mut(),
                    &mut root_entries,
                    &mut root_element,
                    name,
                    id,
                )?;
            }
            Event::End(_) => {
                flush_text(&mut doc, stack.last_mut(), body, base)?;
                let Some(frame) = stack.pop() else {
                    return Err(Error::new(XML_PARSE, "unexpected XML end tag"));
                };
                let id = doc.push(node(
                    Value::Object(frame.entries),
                    &format!("{PREFIX}element"),
                    None,
                    false,
                ))?;
                attach(
                    &mut doc,
                    stack.last_mut(),
                    &mut root_entries,
                    &mut root_element,
                    frame.name,
                    id,
                )?;
            }
            Event::Text(_) | Event::GeneralRef(_) => {
                let Some(frame) = stack.last_mut() else {
                    let raw = std::str::from_utf8(body.get(start..end).unwrap_or_default())
                        .unwrap_or_default();
                    if !is_blank(raw) {
                        return Err(Error::new(XML_PARSE, "text outside the document element"));
                    }
                    continue;
                };
                if frame.text_start.is_none() {
                    frame.text_start = Some(start);
                }
                frame.text_end = end;
            }
            Event::CData(data) => {
                flush_text(&mut doc, stack.last_mut(), body, base)?;
                let raw = data.into_inner();
                let id = doc.push(read_only(
                    Value::Str(raw.into_owned()),
                    &format!("{PREFIX}cdata"),
                ))?;
                add_to_current(
                    &mut doc,
                    stack.last_mut(),
                    &mut root_entries,
                    "#cdata".to_owned(),
                    id,
                )?;
            }
            event @ (Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_)) => {
                flush_text(&mut doc, stack.last_mut(), body, base)?;
                let (key, hint) = match &event {
                    Event::Comment(_) => ("#comment", "comment"),
                    Event::PI(_) => ("#pi", "pi"),
                    Event::Decl(_) => ("#declaration", "declaration"),
                    _ => ("#doctype", "doctype"),
                };
                let value =
                    String::from_utf8_lossy(body.get(start..end).unwrap_or_default()).into_owned();
                let id = doc.push(read_only(Value::Str(value), &format!("{PREFIX}{hint}")))?;
                if stack.is_empty() {
                    append_numbered(
                        &mut doc,
                        &mut root_entries,
                        &format!("{PREFIX}children"),
                        key.to_owned(),
                        id,
                    )?;
                } else {
                    add_to_current(
                        &mut doc,
                        stack.last_mut(),
                        &mut root_entries,
                        key.to_owned(),
                        id,
                    )?;
                }
            }
            Event::Eof => break,
        }
    }
    if let Some(frame) = stack.pop() {
        return Err(Error::new(
            XML_PARSE,
            format!("unclosed XML element <{}>", frame.name),
        ));
    }
    if root_element.is_none() {
        return Err(Error::new(
            XML_PARSE,
            "the XML document has no root element",
        ));
    }
    let root = doc.push(read_only(
        Value::Object(root_entries),
        &format!("{PREFIX}document"),
    ))?;
    doc.set_root(root);
    Ok(doc)
}

fn new_frame(
    doc: &mut SaveDoc,
    name: &str,
    text: &str,
    start: usize,
    end: usize,
    base: usize,
) -> Result<Frame> {
    let raw = text
        .get(start..end)
        .ok_or_else(|| Error::new(XML_PARSE, "invalid XML tag span"))?;
    let mut entries = Vec::new();
    for attribute in attributes(raw, name.len(), start, base)? {
        let id = doc.push(node(
            Value::Str(attribute.value),
            &format!("{PREFIX}attribute"),
            Some((attribute.start, attribute.end)),
            attribute.editable,
        ))?;
        append_numbered(
            doc,
            &mut entries,
            &format!("{PREFIX}attributes"),
            format!("@{}", attribute.name),
            id,
        )?;
    }
    Ok(Frame {
        name: name.to_owned(),
        entries,
        text_start: None,
        text_end: start,
    })
}

struct RawAttribute {
    name: String,
    value: String,
    start: usize,
    end: usize,
    editable: bool,
}

fn attributes(
    raw: &str,
    name_len: usize,
    event_start: usize,
    base: usize,
) -> Result<Vec<RawAttribute>> {
    let bytes = raw.as_bytes();
    let mut at = 1usize.saturating_add(name_len);
    let mut out = Vec::new();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        match bytes.get(at).copied() {
            Some(b'>') | None => break,
            Some(b'/') => {
                at += 1;
                continue;
            }
            _ => {}
        }
        let name_start = at;
        while bytes
            .get(at)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'=' | b'/' | b'>'))
        {
            at += 1;
        }
        let name = raw
            .get(name_start..at)
            .ok_or_else(|| Error::new(XML_PARSE, "invalid XML attribute name"))?
            .to_owned();
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'=') {
            return Err(Error::new(
                XML_PARSE,
                format!("XML attribute {name} has no value"),
            ));
        }
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let Some(quote @ (b'"' | b'\'')) = bytes.get(at).copied() else {
            return Err(Error::new(
                XML_PARSE,
                format!("XML attribute {name} is not quoted"),
            ));
        };
        at += 1;
        let value_start = at;
        while bytes.get(at).is_some_and(|byte| *byte != quote) {
            at += 1;
        }
        if bytes.get(at).is_none() {
            return Err(Error::new(
                XML_PARSE,
                format!("unterminated XML attribute {name}"),
            ));
        }
        let value_end = at;
        at += 1;
        let escaped = raw
            .get(value_start..value_end)
            .ok_or_else(|| Error::new(XML_PARSE, "invalid XML attribute span"))?;
        let absolute = |offset: usize| base.saturating_add(event_start).saturating_add(offset);
        match quick_xml::escape::unescape(escaped) {
            Ok(value) => out.push(RawAttribute {
                name,
                value: value.into_owned(),
                start: absolute(value_start),
                end: absolute(value_end),
                editable: true,
            }),
            Err(_) => out.push(RawAttribute {
                name,
                value: escaped.to_owned(),
                start: absolute(value_start),
                end: absolute(value_end),
                editable: false,
            }),
        }
    }
    Ok(out)
}

fn flush_text(
    doc: &mut SaveDoc,
    frame: Option<&mut Frame>,
    body: &[u8],
    base: usize,
) -> Result<()> {
    let Some(frame) = frame else {
        return Ok(());
    };
    let Some(start) = frame.text_start.take() else {
        return Ok(());
    };
    let end = frame.text_end;
    if end < start || end > body.len() {
        return Err(Error::new(XML_PARSE, "invalid XML text span"));
    }
    let raw = std::str::from_utf8(body.get(start..end).unwrap_or_default())
        .map_err(|_| Error::new(XML_PARSE, "XML text is not UTF-8"))?;
    let start = base.saturating_add(start);
    let end = base.saturating_add(end);
    if is_blank(raw) {
        return Ok(());
    }
    let (value, editable) = match quick_xml::escape::unescape(raw) {
        Ok(value) => (value.into_owned(), true),
        Err(_) => (raw.to_owned(), false),
    };
    let id = doc.push(node(
        Value::Str(value),
        &format!("{PREFIX}text"),
        Some((start, end)),
        editable,
    ))?;
    append_numbered(
        doc,
        &mut frame.entries,
        &format!("{PREFIX}children"),
        "#text".to_owned(),
        id,
    )?;
    Ok(())
}

fn attach(
    doc: &mut SaveDoc,
    parent: Option<&mut Frame>,
    root_entries: &mut Vec<(String, NodeId)>,
    root_element: &mut Option<NodeId>,
    name: String,
    id: NodeId,
) -> Result<()> {
    if let Some(frame) = parent {
        append_numbered(
            doc,
            &mut frame.entries,
            &format!("{PREFIX}children"),
            name,
            id,
        )
    } else {
        if root_element.is_some() {
            return Err(Error::new(
                XML_PARSE,
                "the XML document has more than one root element",
            ));
        }
        *root_element = Some(id);
        append_numbered(doc, root_entries, &format!("{PREFIX}children"), name, id)
    }
}

fn add_to_current(
    doc: &mut SaveDoc,
    parent: Option<&mut Frame>,
    root_entries: &mut Vec<(String, NodeId)>,
    key: String,
    id: NodeId,
) -> Result<()> {
    if let Some(frame) = parent {
        append_numbered(
            doc,
            &mut frame.entries,
            &format!("{PREFIX}children"),
            key,
            id,
        )
    } else {
        append_numbered(doc, root_entries, &format!("{PREFIX}children"), key, id)
    }
}

fn name_of(raw: &str) -> Result<String> {
    Ok(raw.to_owned())
}
