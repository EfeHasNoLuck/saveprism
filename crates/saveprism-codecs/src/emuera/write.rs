//! Writes an Emuera 1.808+ binary save in the engine's canonical encoding.

use saveprism_core::{Error, NodeId, Params, Result, SaveDoc, Value};

use crate::text_encoding::TextEncoding;

use super::{EMUERA_WRITE, HEADER, HINT_PREFIX, MAX_ARRAY_ELEMENTS, VERSION};

const TYPE_INT: u8 = 0x00;
const TYPE_INT_ARRAY: u8 = 0x01;
const TYPE_INT_ARRAY_2D: u8 = 0x02;
const TYPE_INT_ARRAY_3D: u8 = 0x03;
const TYPE_STR: u8 = 0x10;
const TYPE_STR_ARRAY: u8 = 0x11;
const TYPE_STR_ARRAY_2D: u8 = 0x12;
const TYPE_STR_ARRAY_3D: u8 = 0x13;
const TYPE_SEPARATOR: u8 = 0xFD;
const TYPE_EOC: u8 = 0xFE;
const TYPE_EOF: u8 = 0xFF;
const MARK_I16: u8 = 0xD0;
const MARK_I32: u8 = 0xD1;
const MARK_I64: u8 = 0xD2;
const MARK_STRING: u8 = 0xD8;
const MARK_EOA1: u8 = 0xE0;
const MARK_EOA2: u8 = 0xE1;
const MARK_ZERO: u8 = 0xF0;
const MARK_ZERO_A1: u8 = 0xF1;
const MARK_ZERO_A2: u8 = 0xF2;

pub(crate) fn encode(doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
    let root = doc.root();
    let file_type = string(doc, root, "file_type")?;
    let game_code = i64_of(doc, root, "game_code")?;
    let game_version = i64_of(doc, root, "game_version")?;
    let description = string(doc, root, "description")?;
    let file_type_byte = match file_type.as_str() {
        "normal" => 0_u8,
        "global" => 1,
        "variables" => 2,
        "character-variables" => 3,
        other => {
            return Err(write_error(format!("unknown Emuera file type '{other}'")));
        }
    };

    let mut out = Vec::new();
    out.extend_from_slice(&HEADER);
    put_u32(&mut out, VERSION);
    put_u32(&mut out, 0);
    out.push(file_type_byte);
    put_i64(&mut out, game_code);
    put_i64(&mut out, game_version);
    write_string(&mut out, &description)?;

    if file_type_byte == 0 || file_type_byte == 3 {
        let characters = entry(doc, root, "characters")
            .ok_or_else(|| write_error("the Emuera character list is missing"))?;
        let items = array(doc, characters)?;
        let count =
            i64::try_from(items.len()).map_err(|_| write_error("too many Emuera characters"))?;
        put_i64(&mut out, count);
        for character in items.iter().copied() {
            write_character(doc, character, &mut out)?;
        }
    }
    if file_type_byte != 3 {
        let variables = entry(doc, root, "variables")
            .ok_or_else(|| write_error("the Emuera variable list is missing"))?;
        write_pairs(doc, variables, &mut out)?;
        out.push(TYPE_EOF);
    } else {
        out.push(TYPE_EOF);
    }
    Ok(out)
}

fn write_character(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let values =
        entry(doc, id, "values").ok_or_else(|| write_error("an Emuera character has no values"))?;
    write_pairs(doc, values, out)?;
    if let Some(user) = entry(doc, id, "user_defined") {
        out.push(TYPE_SEPARATOR);
        write_pairs(doc, user, out)?;
    }
    out.push(TYPE_EOC);
    Ok(())
}

fn write_pairs(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let node = get(doc, id)?;
    let Value::Object(entries) = &node.value else {
        return Err(write_error("an Emuera variable container is not an object"));
    };
    for (key, child) in entries {
        write_pair(doc, key, *child, out)?;
    }
    Ok(())
}

fn write_pair(doc: &SaveDoc, key: &str, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let node = get(doc, id)?;
    let hint = node
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str())
        .ok_or_else(|| write_error(format!("the Emuera variable '{key}' has no type hint")))?;
    let kind = hint
        .strip_prefix(HINT_PREFIX)
        .ok_or_else(|| write_error(format!("the Emuera variable '{key}' has a bad type hint")))?;
    let type_byte = match kind {
        "int" => TYPE_INT,
        "str" => TYPE_STR,
        "int-array" => TYPE_INT_ARRAY,
        "int-array-2d" => TYPE_INT_ARRAY_2D,
        "int-array-3d" => TYPE_INT_ARRAY_3D,
        "str-array" => TYPE_STR_ARRAY,
        "str-array-2d" => TYPE_STR_ARRAY_2D,
        "str-array-3d" => TYPE_STR_ARRAY_3D,
        other => {
            return Err(write_error(format!(
                "the Emuera variable '{key}' has an unknown type '{other}'"
            )));
        }
    };
    out.push(type_byte);
    write_string(out, key)?;
    match kind {
        "int" => {
            let value = int_value(node)?;
            put_int(out, value);
        }
        "str" => match &node.value {
            Value::Str(text) => write_string(out, text)?,
            _ => {
                return Err(write_error(format!(
                    "the Emuera variable '{key}' is not text"
                )));
            }
        },
        "int-array" => write_int_array_1d(doc, id, out)?,
        "int-array-2d" => write_int_array_2d(doc, id, out)?,
        "int-array-3d" => write_int_array_3d(doc, id, out)?,
        "str-array" => write_str_array_1d(doc, id, out)?,
        "str-array-2d" => write_str_array_2d(doc, id, out)?,
        "str-array-3d" => write_str_array_3d(doc, id, out)?,
        _ => return Err(write_error("unknown Emuera array type")),
    }
    Ok(())
}

fn write_int_array_1d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let items = array(doc, id)?;
    check_len(items.len())?;
    put_i32(
        out,
        i32::try_from(items.len()).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zeros = 0_usize;
    for item in items {
        let value = int_value(get(doc, item)?)?;
        if value == 0 {
            zeros = zeros.saturating_add(1);
        } else {
            flush_zeros(out, &mut zeros)?;
            put_int(out, value);
        }
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn write_int_array_2d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let rows = rows(doc, id)?;
    let length1 = match rows.first() {
        Some(first) => int_row(doc, *first)?.len(),
        None => 0,
    };
    check_len(rows.len().saturating_mul(length1))?;
    put_i32(
        out,
        i32::try_from(rows.len()).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length1).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zero_rows = 0_usize;
    for row in rows {
        let values = int_row(doc, row)?;
        if values.iter().all(|value| *value == 0) {
            zero_rows = zero_rows.saturating_add(1);
            continue;
        }
        flush_zero_rows(out, &mut zero_rows)?;
        let mut zeros = 0_usize;
        for value in values {
            if value == 0 {
                zeros = zeros.saturating_add(1);
            } else {
                flush_zeros(out, &mut zeros)?;
                put_int(out, value);
            }
        }
        out.push(MARK_EOA1);
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn write_int_array_3d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let planes = rows(doc, id)?;
    let length1 = match planes.first() {
        Some(first) => rows(doc, *first)?.len(),
        None => 0,
    };
    let mut plane_values: Vec<Vec<Vec<i64>>> = Vec::with_capacity(planes.len());
    let mut length2 = 0_usize;
    for plane in planes.iter().copied() {
        let inner = rows(doc, plane)?;
        if inner.len() != length1 {
            return Err(write_error("an Emuera 3D array has uneven planes"));
        }
        let mut plane_rows = Vec::with_capacity(inner.len());
        for row in inner {
            let values = int_row(doc, row)?;
            if length2 == 0 {
                length2 = values.len();
            } else if values.len() != length2 {
                return Err(write_error("an Emuera 3D array has uneven rows"));
            }
            plane_rows.push(values);
        }
        plane_values.push(plane_rows);
    }
    check_len(
        plane_values
            .len()
            .saturating_mul(length1)
            .saturating_mul(length2),
    )?;
    put_i32(
        out,
        i32::try_from(plane_values.len())
            .map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length1).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length2).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zero_planes = 0_usize;
    for plane in plane_values {
        if plane.iter().flatten().all(|value| *value == 0) {
            zero_planes = zero_planes.saturating_add(1);
            continue;
        }
        flush_zero_planes(out, &mut zero_planes)?;
        let mut zero_rows = 0_usize;
        for row in plane {
            if row.iter().all(|value| *value == 0) {
                zero_rows = zero_rows.saturating_add(1);
                continue;
            }
            flush_zero_rows(out, &mut zero_rows)?;
            let mut zeros = 0_usize;
            for value in row {
                if value == 0 {
                    zeros = zeros.saturating_add(1);
                } else {
                    flush_zeros(out, &mut zeros)?;
                    put_int(out, value);
                }
            }
            out.push(MARK_EOA1);
        }
        out.push(MARK_EOA2);
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn write_str_array_1d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let items = array(doc, id)?;
    check_len(items.len())?;
    put_i32(
        out,
        i32::try_from(items.len()).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zeros = 0_usize;
    for item in items {
        let node = get(doc, item)?;
        let Value::Str(text) = &node.value else {
            return Err(write_error("an Emuera string array holds a non-string"));
        };
        if text.is_empty() {
            zeros = zeros.saturating_add(1);
        } else {
            flush_zeros(out, &mut zeros)?;
            out.push(MARK_STRING);
            write_string(out, text)?;
        }
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn write_str_array_2d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let rows = rows(doc, id)?;
    let length1 = match rows.first() {
        Some(first) => str_row(doc, *first)?.len(),
        None => 0,
    };
    check_len(rows.len().saturating_mul(length1))?;
    put_i32(
        out,
        i32::try_from(rows.len()).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length1).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zero_rows = 0_usize;
    for row in rows {
        let values = str_row(doc, row)?;
        if values.iter().all(String::is_empty) {
            zero_rows = zero_rows.saturating_add(1);
            continue;
        }
        flush_zero_rows(out, &mut zero_rows)?;
        let mut zeros = 0_usize;
        for value in values {
            if value.is_empty() {
                zeros = zeros.saturating_add(1);
            } else {
                flush_zeros(out, &mut zeros)?;
                out.push(MARK_STRING);
                write_string(out, &value)?;
            }
        }
        out.push(MARK_EOA1);
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn write_str_array_3d(doc: &SaveDoc, id: NodeId, out: &mut Vec<u8>) -> Result<()> {
    let planes = rows(doc, id)?;
    let length1 = match planes.first() {
        Some(first) => rows(doc, *first)?.len(),
        None => 0,
    };
    let mut plane_values: Vec<Vec<Vec<String>>> = Vec::with_capacity(planes.len());
    let mut length2 = 0_usize;
    for plane in planes.iter().copied() {
        let inner = rows(doc, plane)?;
        if inner.len() != length1 {
            return Err(write_error("an Emuera 3D array has uneven planes"));
        }
        let mut plane_rows = Vec::with_capacity(inner.len());
        for row in inner {
            let values = str_row(doc, row)?;
            if length2 == 0 {
                length2 = values.len();
            } else if values.len() != length2 {
                return Err(write_error("an Emuera 3D array has uneven rows"));
            }
            plane_rows.push(values);
        }
        plane_values.push(plane_rows);
    }
    check_len(
        plane_values
            .len()
            .saturating_mul(length1)
            .saturating_mul(length2),
    )?;
    put_i32(
        out,
        i32::try_from(plane_values.len())
            .map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length1).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    put_i32(
        out,
        i32::try_from(length2).map_err(|_| write_error("an Emuera array is too long"))?,
    );
    let mut zero_planes = 0_usize;
    for plane in plane_values {
        if plane.iter().flatten().all(String::is_empty) {
            zero_planes = zero_planes.saturating_add(1);
            continue;
        }
        flush_zero_planes(out, &mut zero_planes)?;
        let mut zero_rows = 0_usize;
        for row in plane {
            if row.iter().all(String::is_empty) {
                zero_rows = zero_rows.saturating_add(1);
                continue;
            }
            flush_zero_rows(out, &mut zero_rows)?;
            let mut zeros = 0_usize;
            for value in row {
                if value.is_empty() {
                    zeros = zeros.saturating_add(1);
                } else {
                    flush_zeros(out, &mut zeros)?;
                    out.push(MARK_STRING);
                    write_string(out, &value)?;
                }
            }
            out.push(MARK_EOA1);
        }
        out.push(MARK_EOA2);
    }
    out.push(TYPE_EOF);
    Ok(())
}

fn flush_zeros(out: &mut Vec<u8>, zeros: &mut usize) -> Result<()> {
    if *zeros == 0 {
        return Ok(());
    }
    out.push(MARK_ZERO);
    put_int(
        out,
        i64::try_from(*zeros).map_err(|_| write_error("an Emuera zero run is too long"))?,
    );
    *zeros = 0;
    Ok(())
}

fn flush_zero_rows(out: &mut Vec<u8>, rows: &mut usize) -> Result<()> {
    if *rows == 0 {
        return Ok(());
    }
    out.push(MARK_ZERO_A1);
    put_int(
        out,
        i64::try_from(*rows).map_err(|_| write_error("an Emuera zero-row run is too long"))?,
    );
    *rows = 0;
    Ok(())
}

fn flush_zero_planes(out: &mut Vec<u8>, planes: &mut usize) -> Result<()> {
    if *planes == 0 {
        return Ok(());
    }
    out.push(MARK_ZERO_A2);
    put_int(
        out,
        i64::try_from(*planes).map_err(|_| write_error("an Emuera zero-plane run is too long"))?,
    );
    *planes = 0;
    Ok(())
}

fn rows(doc: &SaveDoc, id: NodeId) -> Result<Vec<NodeId>> {
    array(doc, id)
}

fn int_row(doc: &SaveDoc, id: NodeId) -> Result<Vec<i64>> {
    let items = array(doc, id)?;
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        values.push(int_value(get(doc, item)?)?);
    }
    Ok(values)
}

fn str_row(doc: &SaveDoc, id: NodeId) -> Result<Vec<String>> {
    let items = array(doc, id)?;
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        let node = get(doc, item)?;
        let Value::Str(text) = &node.value else {
            return Err(write_error("an Emuera string array holds a non-string"));
        };
        values.push(text.clone());
    }
    Ok(values)
}

fn check_len(length: usize) -> Result<()> {
    if length > MAX_ARRAY_ELEMENTS {
        return Err(write_error(format!(
            "an Emuera array has more than {MAX_ARRAY_ELEMENTS} elements"
        )));
    }
    Ok(())
}

fn get(doc: &SaveDoc, id: NodeId) -> Result<&saveprism_core::Node> {
    doc.get(id)
        .ok_or_else(|| write_error("the Emuera document has an invalid node"))
}

fn entry(doc: &SaveDoc, id: NodeId, key: &str) -> Option<NodeId> {
    let node = doc.get(id)?;
    let Value::Object(entries) = &node.value else {
        return None;
    };
    entries
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, child)| *child)
}

fn string(doc: &SaveDoc, id: NodeId, key: &str) -> Result<String> {
    let child = entry(doc, id, key)
        .ok_or_else(|| write_error(format!("the Emuera field '{key}' is missing")))?;
    let node = get(doc, child)?;
    match &node.value {
        Value::Str(text) => Ok(text.clone()),
        _ => Err(write_error(format!("the Emuera field '{key}' is not text"))),
    }
}

fn i64_of(doc: &SaveDoc, id: NodeId, key: &str) -> Result<i64> {
    let child = entry(doc, id, key)
        .ok_or_else(|| write_error(format!("the Emuera field '{key}' is missing")))?;
    int_value(get(doc, child)?)
}

fn int_value(node: &saveprism_core::Node) -> Result<i64> {
    match &node.value {
        Value::Int { v, .. } => {
            i64::try_from(*v).map_err(|_| write_error("an Emuera integer does not fit int64"))
        }
        _ => Err(write_error("an Emuera value is not an integer")),
    }
}

fn array(doc: &SaveDoc, id: NodeId) -> Result<Vec<NodeId>> {
    let node = get(doc, id)?;
    match &node.value {
        Value::Array(items) => Ok(items.clone()),
        _ => Err(write_error("an Emuera array is not an array")),
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_int(out: &mut Vec<u8>, value: i64) {
    if (0..=0xCF).contains(&value) {
        out.push(u8::try_from(value).unwrap_or(0xCF));
    } else if let Ok(value) = i16::try_from(value) {
        out.push(MARK_I16);
        out.extend_from_slice(&value.to_le_bytes());
    } else if let Ok(value) = i32::try_from(value) {
        out.push(MARK_I32);
        out.extend_from_slice(&value.to_le_bytes());
    } else {
        out.push(MARK_I64);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_string(out: &mut Vec<u8>, text: &str) -> Result<()> {
    let bytes = TextEncoding::Utf16Le.encode(text)?;
    let mut length = bytes.len();
    loop {
        let mut byte = u8::try_from(length & 0x7F).unwrap_or(0);
        length >>= 7;
        if length != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if length == 0 {
            break;
        }
    }
    out.extend_from_slice(&bytes);
    Ok(())
}

fn write_error(message: impl Into<String>) -> Error {
    Error::new(EMUERA_WRITE, message)
}
