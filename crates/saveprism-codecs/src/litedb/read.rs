//! LiteDB datafile readers (v5 datafile version 8, v4 datafile version 7).

use std::collections::{BTreeMap, HashSet};

use saveprism_core::{
    Error, IntWidth, Limits, Node, NodeFlags, NodeId, Result, SaveDoc, TypeHint, Value,
};

use super::{header_version, parse_error};

const V5_PAGE: usize = 8192;
const V4_PAGE: usize = 4096;
const V4_HEADER_SIZE: usize = 25;
const PAGE_TYPE_COLLECTION: u8 = 2;
const PAGE_TYPE_INDEX: u8 = 3;
const PAGE_TYPE_DATA: u8 = 4;
const PAGE_TYPE_EXTEND: u8 = 5;
const NONE: u32 = u32::MAX;
const MAX_CHAIN: usize = 4096;

const HINT_COLLECTIONS: &str = "LITEDB:collections";
const HINT_COLLECTION: &str = "LITEDB:collection";
const HINT_DOCUMENT: &str = "LITEDB:document";

/// Which values of a collection an index depends on.
///
/// LiteDB index pages keep a copy of every indexed key, so a document field that an index covers
/// cannot be patched without updating the index. Such values are read-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Indexed {
    /// At least one index expression is not a simple `$.Field` path, so every value is read-only.
    All,
    /// Top-level field names (the first path segment) covered by the collection's indexes.
    Fields(Vec<String>),
}

/// Reads a LiteDB file (v4 or v5) into a tree of collections and documents.
pub(super) fn decode(input: &[u8], limits: &Limits) -> Result<SaveDoc> {
    let version = header_version(input)
        .ok_or_else(|| parse_error("the LiteDB header is missing or unknown", 0))?;
    let collections = match version {
        8 => read_v5(input, limits)?,
        7 => read_v4(input, limits)?,
        _ => return Err(parse_error("the LiteDB datafile version is unknown", 0)),
    };
    let plan = index_plan(input, limits)?;
    let mut doc = SaveDoc::with_node_limit(limits.max_nodes);
    let mut root_entries = Vec::new();
    for (name, documents) in collections {
        let indexed = plan.get(&name).cloned().unwrap_or(Indexed::All);
        let mut array = Vec::new();
        for (document, base) in documents {
            let copied = copy_tree(&document, document.root(), &mut doc, base)?;
            doc.set_type_hint(copied, Some(TypeHint::new(HINT_DOCUMENT)))?;
            apply_index_flags(&mut doc, copied, &indexed)?;
            array.push(copied);
        }
        let mut node = Node::new(Value::Array(array));
        node.type_hint = Some(TypeHint::new(HINT_COLLECTION));
        node.flags.insert(NodeFlags::READ_ONLY);
        root_entries.push((name, doc.push(node)?));
    }
    let mut root = Node::new(Value::Object(root_entries));
    root.type_hint = Some(TypeHint::new(HINT_COLLECTIONS));
    root.flags.insert(NodeFlags::READ_ONLY);
    let root = doc.push(root)?;
    doc.set_root(root);
    Ok(doc)
}

/// Builds the per-collection index plan from the header and collection pages.
///
/// A collection whose collection page cannot be read exactly is treated as [`Indexed::All`], so no
/// value of it can be edited.
pub(super) fn index_plan(input: &[u8], limits: &Limits) -> Result<BTreeMap<String, Indexed>> {
    let version = header_version(input)
        .ok_or_else(|| parse_error("the LiteDB header is missing or unknown", 0))?;
    let mut plan = BTreeMap::new();
    match version {
        8 => {
            for (name, page) in header_collections_v5(input, limits)? {
                plan.insert(name, v5_indexed(input, page));
            }
        }
        7 => {
            for (name, page) in header_collections_v4(input)? {
                plan.insert(name, v4_indexed(input, page));
            }
        }
        _ => return Err(parse_error("the LiteDB datafile version is unknown", 0)),
    }
    Ok(plan)
}

/// The top-level field of a simple LiteDB index path (`$.Field` or `$.Field.Sub`), if any.
pub(super) fn simple_first(expression: &str) -> Option<String> {
    let rest = expression.strip_prefix("$.")?;
    if rest.is_empty() {
        return None;
    }
    let mut first = None;
    for segment in rest.split('.') {
        if segment.is_empty()
            || !segment
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_')
        {
            return None;
        }
        if first.is_none() {
            first = Some(segment.to_string());
        }
    }
    first
}

fn fields_from_expressions<'a>(expressions: impl Iterator<Item = &'a str>) -> Indexed {
    let mut fields = Vec::new();
    for expression in expressions {
        match simple_first(expression) {
            Some(field) => fields.push(field),
            None => return Indexed::All,
        }
    }
    Indexed::Fields(fields)
}

fn v5_indexed(input: &[u8], collection_page: u32) -> Indexed {
    let Ok(offset) = page_offset(collection_page, V5_PAGE) else {
        return Indexed::All;
    };
    let Some(page) = input.get(offset..offset.saturating_add(V5_PAGE)) else {
        return Indexed::All;
    };
    if page.get(4).copied() != Some(PAGE_TYPE_COLLECTION)
        || u32_at(page, 0) != Some(collection_page)
    {
        return Indexed::All;
    }
    let Some(count) = page.get(96).copied() else {
        return Indexed::All;
    };
    let mut cursor = 97_usize;
    let mut expressions = Vec::new();
    for _ in 0..count {
        // Slot, index type, name, expression, unique, head (5), tail (5), reserved, free list (4).
        cursor = cursor.saturating_add(2);
        let Some((_name, after_name)) = cstring_at(page, cursor) else {
            return Indexed::All;
        };
        let Some((expression, after_expression)) = cstring_at(page, after_name) else {
            return Indexed::All;
        };
        cursor = after_expression.saturating_add(16);
        if cursor > V5_PAGE {
            return Indexed::All;
        }
        expressions.push(expression);
    }
    fields_from_expressions(expressions.iter().map(String::as_str))
}

fn v4_indexed(input: &[u8], collection_page: u32) -> Indexed {
    let Ok(offset) = page_offset(collection_page, V4_PAGE) else {
        return Indexed::All;
    };
    let Some(page) = input.get(offset..offset.saturating_add(V4_PAGE)) else {
        return Indexed::All;
    };
    if page.get(4).copied() != Some(PAGE_TYPE_COLLECTION)
        || u32_at(page, 0) != Some(collection_page)
    {
        return Indexed::All;
    }
    let Ok(name) = v4_cstring(page, V4_HEADER_SIZE) else {
        return Indexed::All;
    };
    let Some(mut cursor) = V4_HEADER_SIZE
        .checked_add(name.len())
        .and_then(|size| size.checked_add(1))
        .and_then(|size| size.checked_add(12))
    else {
        return Indexed::All;
    };
    let mut expressions = Vec::new();
    for _slot in 0..16_u8 {
        let Ok(field) = v4_cstring(page, cursor) else {
            return Indexed::All;
        };
        // Field name + NUL, unique byte, head page id (4), then the skipped head/tail/free list.
        let Some(next) = cursor
            .checked_add(field.len())
            .and_then(|size| size.checked_add(1 + 1 + 4 + 12))
        else {
            return Indexed::All;
        };
        cursor = next;
        if field.is_empty() {
            continue;
        }
        match field.split_once('=') {
            Some((_, expression)) => expressions.push(expression.to_string()),
            None => expressions.push(format!("$.{field}")),
        }
    }
    fields_from_expressions(expressions.iter().map(String::as_str))
}

fn apply_index_flags(doc: &mut SaveDoc, document: NodeId, indexed: &Indexed) -> Result<()> {
    let entries = match doc.get(document).map(|node| &node.value) {
        Some(Value::Object(entries)) => entries.clone(),
        _ => return Ok(()),
    };
    for (key, child) in entries {
        let covered = key == "_id"
            || match indexed {
                Indexed::All => true,
                Indexed::Fields(fields) => fields.iter().any(|field| field == &key),
            };
        if covered {
            mark_subtree(doc, child)?;
        }
    }
    Ok(())
}

fn mark_subtree(doc: &mut SaveDoc, root: NodeId) -> Result<()> {
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = doc.get(id) else {
            continue;
        };
        let mut flags = node.flags;
        flags.insert(NodeFlags::READ_ONLY);
        flags.insert(NodeFlags::INDEXED);
        let children: Vec<NodeId> = match &node.value {
            Value::Array(items) => items.clone(),
            Value::Object(entries) => entries.iter().map(|(_, child)| *child).collect(),
            _ => Vec::new(),
        };
        doc.set_flags(id, flags)?;
        stack.extend(children);
    }
    Ok(())
}

fn page_offset(page: u32, size: usize) -> Result<usize> {
    usize::try_from(page)
        .unwrap_or(usize::MAX)
        .checked_mul(size)
        .ok_or_else(|| parse_error("a LiteDB page offset overflows", 0))
}

fn cstring_at(input: &[u8], at: usize) -> Option<(String, usize)> {
    let rest = input.get(at..)?;
    let length = rest.iter().position(|byte| *byte == 0)?;
    let text = rest.get(..length)?;
    let text = std::str::from_utf8(text).ok()?.to_string();
    Some((text, at.checked_add(length + 1)?))
}

/// One document plus whether its scalar spans are usable (for in-place patches).
type Document = (SaveDoc, bool);

fn read_v5(input: &[u8], limits: &Limits) -> Result<Vec<(String, Vec<Document>)>> {
    if input.len() < V5_PAGE {
        return Err(parse_error(
            "a LiteDB v5 file is at least one 8192-byte page",
            0,
        ));
    }
    let pages = input.len() / V5_PAGE;
    let last =
        u32_at(input, 64).ok_or_else(|| parse_error("the header last-page id is cut short", 64))?;
    let last = usize::try_from(last).unwrap_or(usize::MAX);
    if last >= pages {
        return Err(parse_error(
            "the file is shorter than the last page id in its header",
            input.len(),
        ));
    }
    let collections = header_collections_v5(input, limits)?;
    let mut result = Vec::new();
    for (name, collection_page) in collections {
        let mut documents = Vec::new();
        for page_id in 1..=last {
            let offset = page_id
                .checked_mul(V5_PAGE)
                .ok_or_else(|| parse_error("the page offset overflows", 0))?;
            let page = input
                .get(offset..offset.saturating_add(V5_PAGE))
                .ok_or_else(|| parse_error("a data page is cut short", offset))?;
            if page.get(4).copied() != Some(PAGE_TYPE_DATA) {
                continue;
            }
            let col = u32_at(page, 19).unwrap_or(NONE);
            if col != collection_page {
                continue;
            }
            collect_v5_page(input, page, offset, limits, &mut documents)?;
        }
        result.push((name, documents));
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(result)
}

fn header_collections_v5(input: &[u8], limits: &Limits) -> Result<Vec<(String, u32)>> {
    let area = input
        .get(192..8192.min(input.len()))
        .ok_or_else(|| parse_error("the header collections area is cut short", 192))?;
    let parsed = crate::bson::read::parse_embedded(area, 0, limits, None).map_err(map_bson)?;
    let root = parsed.root();
    let Some(node) = parsed.get(root) else {
        return Err(parse_error(
            "the header collections document is missing",
            192,
        ));
    };
    let Value::Object(entries) = &node.value else {
        return Err(parse_error(
            "the header collections value is not a document",
            192,
        ));
    };
    let mut result = Vec::new();
    for (name, id) in entries {
        let Some(value) = parsed.get(*id) else {
            continue;
        };
        if let Value::Int { v, .. } = value.value {
            let page = u32::try_from(v)
                .map_err(|_| parse_error("a collection page id is out of range", 192))?;
            result.push((name.clone(), page));
        }
    }
    Ok(result)
}

fn collect_v5_page(
    input: &[u8],
    page: &[u8],
    page_offset: usize,
    limits: &Limits,
    documents: &mut Vec<Document>,
) -> Result<()> {
    let highest = page.get(30).copied().unwrap_or(255);
    if highest == 255 {
        return Ok(());
    }
    for index in 0..=highest {
        let (position, length) = v5_slot(page, index)?;
        if position == 0 {
            continue;
        }
        let end = position
            .checked_add(length)
            .ok_or_else(|| parse_error("a data segment overflows", page_offset))?;
        let segment = page
            .get(position..end)
            .ok_or_else(|| parse_error("a data segment is cut short", page_offset + position))?;
        if segment.first().copied().unwrap_or(0) != 0 {
            continue;
        }
        let next = page_address(segment, 1);
        let first = segment.get(6..).unwrap_or_default();
        let (bytes, base) = if next.0 == NONE {
            (
                first.to_vec(),
                Some(
                    u64::try_from(page_offset).unwrap_or(0)
                        + u64::try_from(position).unwrap_or(0)
                        + 6,
                ),
            )
        } else {
            let mut bytes = first.to_vec();
            follow_v5(input, next, &mut bytes)?;
            (bytes, None)
        };
        let document = parse_document(&bytes, base, limits)?;
        documents.push((document, base.is_some()));
    }
    Ok(())
}

fn v5_slot(page: &[u8], index: u8) -> Result<(usize, usize)> {
    let slot_end = V5_PAGE
        .checked_sub(usize::from(index).saturating_add(1).saturating_mul(4))
        .ok_or_else(|| parse_error("the data slot index is out of range", 0))?;
    let position = u16_at(page, slot_end + 2)
        .ok_or_else(|| parse_error("a data slot position is cut short", slot_end + 2))?;
    let length = u16_at(page, slot_end)
        .ok_or_else(|| parse_error("a data slot length is cut short", slot_end))?;
    Ok((usize::from(position), usize::from(length)))
}

fn follow_v5(input: &[u8], mut next: (u32, u8), bytes: &mut Vec<u8>) -> Result<()> {
    let mut visited = HashSet::new();
    for _ in 0..MAX_CHAIN {
        if next.0 == NONE {
            return Ok(());
        }
        if !visited.insert(next.0) {
            return Err(parse_error("a data block chain loops", 0));
        }
        let offset = usize::try_from(next.0)
            .unwrap_or(usize::MAX)
            .checked_mul(V5_PAGE)
            .ok_or_else(|| parse_error("a data page offset overflows", 0))?;
        let page = input
            .get(offset..offset.saturating_add(V5_PAGE))
            .ok_or_else(|| parse_error("a continuation page is cut short", offset))?;
        if page.get(4).copied() != Some(PAGE_TYPE_DATA) {
            return Err(parse_error(
                "a continuation page is not a data page",
                offset,
            ));
        }
        let (position, length) = v5_slot(page, next.1)?;
        if position == 0 {
            return Err(parse_error("a continuation block slot is empty", offset));
        }
        let end = position
            .checked_add(length)
            .ok_or_else(|| parse_error("a continuation block overflows", offset))?;
        let segment = page
            .get(position..end)
            .ok_or_else(|| parse_error("a continuation block is cut short", offset))?;
        if segment.first().copied().unwrap_or(0) == 0 {
            return Err(parse_error(
                "a continuation block is not marked extend",
                offset,
            ));
        }
        bytes.extend_from_slice(segment.get(6..).unwrap_or_default());
        next = page_address(segment, 1);
    }
    Err(parse_error("a data block chain is too long", 0))
}

fn read_v4(input: &[u8], limits: &Limits) -> Result<Vec<(String, Vec<Document>)>> {
    if input.len() < V4_PAGE {
        return Err(parse_error(
            "a LiteDB v4 file is at least one 4096-byte page",
            0,
        ));
    }
    let collections = header_collections_v4(input)?;
    let mut result = Vec::new();
    for (name, collection_page) in collections {
        let documents = read_v4_collection(input, collection_page, limits)?;
        result.push((name, documents));
    }
    result.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(result)
}

fn header_collections_v4(input: &[u8]) -> Result<Vec<(String, u32)>> {
    let header = input
        .get(..V4_PAGE)
        .ok_or_else(|| parse_error("the v4 header page is cut short", 0))?;
    let mut reader = crate::kv_common::Reader::new(header, 103)
        .ok_or_else(|| parse_error("the v4 collections area is missing", 103))?;
    let count = reader
        .byte()
        .map_err(|error| parse_error(error.message, error.offset))?;
    let mut result = Vec::new();
    for _ in 0..count {
        let name_start = reader.pos();
        let name = v4_cstring(header, name_start)?;
        let skip = name
            .len()
            .checked_add(1)
            .ok_or_else(|| parse_error("the collection name is too long", name_start))?;
        reader = crate::kv_common::Reader::new(header, name_start.saturating_add(skip))
            .ok_or_else(|| parse_error("the v4 collection name is cut short", name_start))?;
        let page = reader
            .u32_le()
            .map_err(|error| parse_error(error.message, error.offset))?;
        result.push((name, page));
    }
    Ok(result)
}

fn read_v4_collection(
    input: &[u8],
    collection_page: u32,
    limits: &Limits,
) -> Result<Vec<Document>> {
    let offset = v4_page_offset(collection_page)?;
    let page = input
        .get(offset..offset.saturating_add(V4_PAGE))
        .ok_or_else(|| parse_error("the v4 collection page is cut short", offset))?;
    if page.get(4).copied() != Some(PAGE_TYPE_COLLECTION) {
        return Err(parse_error(
            "the v4 collection page has the wrong type",
            offset,
        ));
    }
    let name = v4_cstring(page, V4_HEADER_SIZE)?;
    let mut cursor = V4_HEADER_SIZE
        .checked_add(name.len())
        .and_then(|size| size.checked_add(1))
        .and_then(|size| size.checked_add(12))
        .ok_or_else(|| parse_error("the v4 collection header overflows", offset))?;
    let mut head = NONE;
    for _slot in 0..16_u8 {
        let field = v4_cstring(page, cursor)?;
        cursor = cursor
            .checked_add(field.len())
            .and_then(|size| size.checked_add(1))
            .ok_or_else(|| parse_error("the v4 index list overflows", offset))?;
        let unique = page.get(cursor).copied().unwrap_or(0);
        cursor += 1;
        let head_page = u32_at(page, cursor)
            .ok_or_else(|| parse_error("a v4 index head is cut short", offset + cursor))?;
        cursor = cursor
            .checked_add(4 + 12)
            .ok_or_else(|| parse_error("the v4 index list overflows", offset))?;
        let _ = unique;
        if head == NONE && !field.is_empty() {
            head = head_page;
        }
    }
    if head == NONE {
        return Ok(Vec::new());
    }
    let references = v4_index_references(input, head)?;
    let mut documents = Vec::new();
    for (page_id, index) in references {
        if let Some((bytes, base)) = v4_document(input, page_id, index)? {
            let document = parse_document(&bytes, base, limits)?;
            documents.push((document, base.is_some()));
        }
    }
    Ok(documents)
}

fn v4_index_references(input: &[u8], head: u32) -> Result<Vec<(u32, u16)>> {
    let mut queue = vec![head];
    let mut visited = HashSet::new();
    let mut references = Vec::new();
    while let Some(page_id) = queue.pop() {
        if page_id == NONE || !visited.insert(page_id) {
            continue;
        }
        if visited.len() > MAX_CHAIN {
            return Err(parse_error("the v4 index page tree is too large", 0));
        }
        let offset = v4_page_offset(page_id)?;
        let page = input
            .get(offset..offset.saturating_add(V4_PAGE))
            .ok_or_else(|| parse_error("a v4 index page is cut short", offset))?;
        if page.get(4).copied() != Some(PAGE_TYPE_INDEX) {
            continue;
        }
        let count = usize::from(
            u16_at(page, 13).ok_or_else(|| parse_error("a v4 index page is cut short", offset))?,
        );
        let mut reader = crate::kv_common::Reader::new(page, V4_HEADER_SIZE)
            .ok_or_else(|| parse_error("the v4 index area is missing", offset))?;
        for _ in 0..count {
            let _index = reader
                .u16_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let levels = reader
                .byte()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let _slot = reader
                .byte()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let prev_page = reader
                .u32_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let _prev_index = reader
                .u16_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let next_page = reader
                .u32_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let _next_index = reader
                .u16_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let key_len = reader
                .u16_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let _kind = reader
                .byte()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            reader
                .take(usize::from(key_len))
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let data_page = reader
                .u32_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let data_index = reader
                .u16_le()
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            reader
                .take(12)
                .map_err(|error| parse_error(error.message, offset + error.offset))?;
            let extra = usize::from(levels.saturating_sub(1)).saturating_mul(12);
            if extra > 0 {
                reader
                    .take(extra)
                    .map_err(|error| parse_error(error.message, offset + error.offset))?;
            }
            if data_page != NONE {
                references.push((data_page, data_index));
            }
            if prev_page != NONE {
                queue.push(prev_page);
            }
            if next_page != NONE {
                queue.push(next_page);
            }
        }
    }
    Ok(references)
}

fn v4_document(input: &[u8], page_id: u32, index: u16) -> Result<Option<(Vec<u8>, Option<u64>)>> {
    let offset = v4_page_offset(page_id)?;
    let page = input
        .get(offset..offset.saturating_add(V4_PAGE))
        .ok_or_else(|| parse_error("a v4 data page is cut short", offset))?;
    if page.get(4).copied() != Some(PAGE_TYPE_DATA) {
        return Ok(None);
    }
    let count = usize::from(
        u16_at(page, 13).ok_or_else(|| parse_error("a v4 data page is cut short", offset))?,
    );
    let mut reader = crate::kv_common::Reader::new(page, V4_HEADER_SIZE)
        .ok_or_else(|| parse_error("the v4 data area is missing", offset))?;
    for _ in 0..count {
        let block_index = reader
            .u16_le()
            .map_err(|error| parse_error(error.message, offset + error.offset))?;
        let extend_page = reader
            .u32_le()
            .map_err(|error| parse_error(error.message, offset + error.offset))?;
        let length = reader
            .u16_le()
            .map_err(|error| parse_error(error.message, offset + error.offset))?;
        let data_start = reader.pos();
        let data = reader
            .take(usize::from(length))
            .map_err(|error| parse_error(error.message, offset + error.offset))?;
        if block_index != index {
            continue;
        }
        if extend_page == NONE {
            let base =
                Some(u64::try_from(offset).unwrap_or(0) + u64::try_from(data_start).unwrap_or(0));
            return Ok(Some((data.to_vec(), base)));
        }
        let mut bytes = data.to_vec();
        follow_v4(input, extend_page, &mut bytes)?;
        return Ok(Some((bytes, None)));
    }
    Ok(None)
}

fn follow_v4(input: &[u8], mut page_id: u32, bytes: &mut Vec<u8>) -> Result<()> {
    let mut visited = HashSet::new();
    for _ in 0..MAX_CHAIN {
        if page_id == NONE {
            return Ok(());
        }
        if !visited.insert(page_id) {
            return Err(parse_error("a v4 extend page chain loops", 0));
        }
        let offset = v4_page_offset(page_id)?;
        let page = input
            .get(offset..offset.saturating_add(V4_PAGE))
            .ok_or_else(|| parse_error("a v4 extend page is cut short", offset))?;
        if page.get(4).copied() != Some(PAGE_TYPE_EXTEND) {
            return Err(parse_error("a v4 extend page has the wrong type", offset));
        }
        let count = usize::from(
            u16_at(page, 13).ok_or_else(|| parse_error("a v4 extend page is cut short", offset))?,
        );
        let chunk = page
            .get(V4_HEADER_SIZE..V4_HEADER_SIZE.saturating_add(count))
            .ok_or_else(|| parse_error("a v4 extend chunk is cut short", offset))?;
        bytes.extend_from_slice(chunk);
        page_id = u32_at(page, 9)
            .ok_or_else(|| parse_error("a v4 extend page has no next id", offset))?;
    }
    Err(parse_error("a v4 extend chain is too long", 0))
}

fn parse_document(bytes: &[u8], base: Option<u64>, limits: &Limits) -> Result<SaveDoc> {
    let declared = bytes
        .get(..4)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(i32::from_le_bytes)
        .ok_or_else(|| parse_error("a document has no length", 0))?;
    if declared < 5 {
        return Err(parse_error("a document length is smaller than 5", 0));
    }
    let length =
        usize::try_from(declared).map_err(|_| parse_error("a document length is too large", 0))?;
    if length > bytes.len() {
        return Err(parse_error(
            "a document extends past its data block",
            bytes.len(),
        ));
    }
    let slice = bytes.get(..length).unwrap_or(bytes);
    crate::bson::read::parse_embedded(slice, 0, limits, base).map_err(map_bson)
}

fn map_bson(error: Error) -> Error {
    parse_error(
        error.message(),
        usize::try_from(error.offset().unwrap_or(0)).unwrap_or(0),
    )
}

fn copy_tree(from: &SaveDoc, id: NodeId, to: &mut SaveDoc, spans: bool) -> Result<NodeId> {
    let node = from
        .get(id)
        .ok_or_else(|| parse_error("a document node is missing", 0))?;
    let mut copy = Node::new(Value::Null);
    copy.type_hint = node.type_hint.clone();
    copy.span = node.span;
    match &node.value {
        Value::Array(items) => {
            let mut copied = Vec::with_capacity(items.len());
            for item in items {
                copied.push(copy_tree(from, *item, to, spans)?);
            }
            copy.value = Value::Array(copied);
            copy.flags.insert(NodeFlags::READ_ONLY);
        }
        Value::Object(entries) => {
            let mut copied = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                copied.push((key.clone(), copy_tree(from, *item, to, spans)?));
            }
            copy.value = Value::Object(copied);
            copy.flags.insert(NodeFlags::READ_ONLY);
        }
        value => {
            copy.value = value.clone();
            let fixed = spans
                && copy.span.is_some()
                && matches!(
                    value,
                    Value::Bool(_)
                        | Value::Int {
                            width: IntWidth::I32 | IntWidth::I64,
                            ..
                        }
                        | Value::Float { .. }
                );
            if !fixed {
                copy.flags.insert(NodeFlags::READ_ONLY);
            }
        }
    }
    to.push(copy)
}

fn page_address(segment: &[u8], at: usize) -> (u32, u8) {
    let page = u32_at(segment, at).unwrap_or(NONE);
    let index = segment.get(at.saturating_add(4)).copied().unwrap_or(255);
    (page, index)
}

fn u16_at(input: &[u8], at: usize) -> Option<u16> {
    let bytes: [u8; 2] = input.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn u32_at(input: &[u8], at: usize) -> Option<u32> {
    let bytes: [u8; 4] = input.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn v4_page_offset(page: u32) -> Result<usize> {
    usize::try_from(page)
        .unwrap_or(usize::MAX)
        .checked_mul(V4_PAGE)
        .ok_or_else(|| parse_error("a v4 page offset overflows", 0))
}

fn v4_cstring(input: &[u8], at: usize) -> Result<String> {
    let rest = input
        .get(at..)
        .ok_or_else(|| parse_error("a v4 string is cut short", at))?;
    let length = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or_else(|| parse_error("a v4 string has no terminator", at))?;
    let text = rest
        .get(..length)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .ok_or_else(|| parse_error("a v4 string is not UTF-8", at))?;
    Ok(text.to_string())
}
