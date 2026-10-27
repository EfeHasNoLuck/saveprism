//! Finds the struct type hints a GVAS file needs.
//!
//! A `MapProperty` or `SetProperty` only stores "StructProperty" for a struct
//! key or value; the actual layout is not in the file. `uesave` falls back to
//! `Guid` for map keys and set elements and to a plain property-list struct for
//! map values, and a wrong fallback can either fail outright or read the wrong
//! number of bytes and desync the rest of the file. This module retries with
//! candidate struct types at the paths `uesave` consults (`<path>.Key`,
//! `<path>.Value`, or `<path>` for a set) and keeps a candidate only when the
//! whole file decodes strictly and writes back byte for byte.
//!
//! Two sources of candidate paths are used:
//! - a lenient parse (`error_to_raw`) whose unreadable properties still appear
//!   as raw nodes with their recorded schema, giving exact dotted paths;
//! - a scan of the raw bytes for property-name/type-string pairs, used when a
//!   wrong guess desyncs the parse before anything can be recorded. The scan is
//!   only a hint proposer: every proposal is still verified by a real parse.
//!
//! The search is bounded by attempts, depth and wall-clock time. It is only
//! started after a normal strict parse fails, so files that already decode are
//! unaffected.

use std::time::{Duration, Instant};

use saveprism_core::{Error, ParamValue, Params, Result};
use uesave::{
    MapEntry, Properties, Property, PropertySchemas, PropertyTagDataPartial, Save, StructType,
    StructValue, Types, ValueVec,
};

use super::{GVAS_PARSE, guarded, parse_save, parse_save_lenient, write_save};

/// Candidate struct types, the plain property-list struct first, then `Guid`,
/// then the remaining fixed-size types `uesave` knows.
const CANDIDATES: &[&str] = &[
    "Struct",
    "Guid",
    "DateTime",
    "Timespan",
    "Vector2D",
    "Vector",
    "Vector4",
    "IntVector",
    "Box",
    "Box2D",
    "IntPoint",
    "Quat",
    "Rotator",
    "LinearColor",
    "Color",
    "SoftObjectPath",
    "SoftClassPath",
    "GameplayTagContainer",
    "UniqueNetIdRepl",
    "KeyHandleMap",
    "RichCurveKey",
    "SkeletalMeshSamplingLODBuiltData",
    "PerPlatformFloat",
];

/// Total retries (each is one parse) allowed for one file.
const MAX_ATTEMPTS: u32 = 64;
/// Maximum number of hints added on one branch above the user's table.
const MAX_DEPTH: usize = 8;
/// Wall-clock budget for the whole search.
const MAX_TIME: Duration = Duration::from_secs(3);

/// Property type strings worth finding in the raw tag stream.
const TYPE_NAMES: &[&[u8]] = &[
    b"MapProperty",
    b"SetProperty",
    b"StructProperty",
    b"ArrayProperty",
];

const MAP: &[u8] = b"MapProperty";
const SET: &[u8] = b"SetProperty";

/// One component of an unresolved map/set whose type is a struct.
struct Component {
    path: String,
    current: StructType,
}

/// An unresolved map or set found in a lenient parse.
struct Group {
    path: String,
    components: Vec<Component>,
}

/// A property tag found by scanning the raw bytes.
struct Scanned {
    pos: usize,
    name: String,
    kind: &'static [u8],
}

/// Builds a `uesave` type table from a validated `types` parameter table.
pub(super) fn types_from(table: &Params) -> Types {
    let mut types = Types::new();
    for (path, value) in table.iter() {
        if let ParamValue::Str(name) = value {
            types.add(path.to_string(), StructType::from(name.as_str()));
        }
    }
    types
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Turns the schema of an unresolved property into hint components.
fn group_from_schema(tag: &PropertyTagDataPartial, path: &str, out: &mut Vec<Group>) {
    match tag {
        PropertyTagDataPartial::Map {
            key_type,
            value_type,
        } => {
            let mut components = Vec::new();
            if let PropertyTagDataPartial::Struct { struct_type, .. } = &**key_type {
                components.push(Component {
                    path: format!("{path}.Key"),
                    current: struct_type.clone(),
                });
            }
            if let PropertyTagDataPartial::Struct { struct_type, .. } = &**value_type {
                components.push(Component {
                    path: format!("{path}.Value"),
                    current: struct_type.clone(),
                });
            }
            if !components.is_empty() {
                out.push(Group {
                    path: path.to_string(),
                    components,
                });
            }
        }
        PropertyTagDataPartial::Set { key_type } => {
            if let PropertyTagDataPartial::Struct { struct_type, .. } = &**key_type {
                out.push(Group {
                    path: path.to_string(),
                    components: vec![Component {
                        path: path.to_string(),
                        current: struct_type.clone(),
                    }],
                });
            }
        }
        _ => {}
    }
}

fn scan_properties(
    properties: &Properties,
    prefix: &str,
    schemas: &PropertySchemas,
    out: &mut Vec<Group>,
) {
    for (key, property) in properties.0.iter() {
        let path = join(prefix, &key.1);
        scan_property(property, &path, schemas, out);
    }
}

fn scan_property(property: &Property, path: &str, schemas: &PropertySchemas, out: &mut Vec<Group>) {
    match property {
        Property::Raw(_) => {
            if let Some(tag) = schemas.get(path) {
                group_from_schema(&tag.data, path, out);
            }
        }
        Property::Struct(StructValue::Struct(inner)) => {
            scan_properties(inner, path, schemas, out);
        }
        Property::Set(ValueVec::Struct(items)) | Property::Array(ValueVec::Struct(items)) => {
            for item in items {
                if let StructValue::Struct(inner) = item {
                    scan_properties(inner, path, schemas, out);
                }
            }
        }
        Property::Map(entries) => {
            for MapEntry { key, value } in entries {
                scan_property(key, path, schemas, out);
                scan_property(value, path, schemas, out);
            }
        }
        _ => {}
    }
}

/// Collects the unresolved map/set paths of a lenient parse in document order.
fn unresolved(save: &Save) -> Vec<Group> {
    let mut groups = Vec::new();
    scan_properties(&save.root.properties, "", &save.schemas, &mut groups);
    groups
}

/// Reads a little-endian `i32` at `pos`.
fn read_i32(input: &[u8], pos: usize) -> Option<i32> {
    let bytes = input.get(pos..pos.checked_add(4)?)?;
    let array: [u8; 4] = bytes.try_into().ok()?;
    Some(i32::from_le_bytes(array))
}

/// Reads the property name that precedes a property type string at `type_pos`.
fn name_before(input: &[u8], type_pos: usize) -> Option<String> {
    let type_len = read_i32(input, type_pos.checked_sub(4)?)?;
    if type_len <= 0 {
        return None;
    }
    let name_end = type_pos.checked_sub(4)?;
    for name_len in 1..=256_i32 {
        let name_start = name_end.checked_sub(4 + usize::try_from(name_len).ok()?)?;
        if read_i32(input, name_start)? != name_len {
            continue;
        }
        let bytes = input.get(name_start + 4..name_end)?;
        let (last, body) = bytes.split_last()?;
        if *last != 0 || body.is_empty() || body.contains(&0) {
            continue;
        }
        let ascii = body
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ');
        if ascii {
            return String::from_utf8(body.to_vec()).ok();
        }
    }
    None
}

/// Finds property-name/type-string pairs in the raw bytes.
fn scan_tags(input: &[u8]) -> Vec<Scanned> {
    let mut found = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        let Some(rest) = input.get(pos..) else {
            break;
        };
        let mut matched = false;
        for needle in TYPE_NAMES {
            if rest.starts_with(needle) {
                if let Some(name) = name_before(input, pos) {
                    found.push(Scanned {
                        pos,
                        name,
                        kind: needle,
                    });
                }
                pos += needle.len();
                matched = true;
                break;
            }
        }
        if !matched {
            pos += 1;
        }
    }
    found
}

/// Candidate hint paths read from the raw tags, closest to the strict parse
/// error first. Plain root paths come before one-level prefixed paths.
fn scanned_paths(input: &[u8], error_offset: usize) -> Vec<String> {
    let mut tags = scan_tags(input);
    tags.sort_by_key(|tag| tag.pos.abs_diff(error_offset));

    let mut paths = Vec::new();
    for tag in &tags {
        match tag.kind {
            SET => paths.push(tag.name.clone()),
            MAP => {
                paths.push(format!("{}.Key", tag.name));
                paths.push(format!("{}.Value", tag.name));
            }
            _ => {}
        }
    }

    let names: Vec<&Scanned> = tags.iter().collect();
    for tag in &tags {
        let suffix = match tag.kind {
            SET => vec![tag.name.clone()],
            MAP => vec![format!("{}.Key", tag.name), format!("{}.Value", tag.name)],
            _ => continue,
        };
        for other in names.iter().filter(|other| other.pos < tag.pos).take(8) {
            for item in &suffix {
                paths.push(format!("{}.{item}", other.name));
            }
        }
    }
    paths.truncate(64);

    let mut unique = Vec::new();
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    unique
}

struct Search<'a> {
    input: &'a [u8],
    attempts: u32,
    deadline: Instant,
}

impl Search<'_> {
    fn retry(&mut self, table: Params, depth: usize) -> Option<(Save, Params)> {
        if self.attempts == 0 || Instant::now() >= self.deadline {
            return None;
        }
        self.attempts -= 1;
        self.run(table, depth)
    }

    /// Tries the current table, then one more hint per level. Hints found here
    /// must give an exact round trip: a candidate is kept only if the final
    /// strict parse and write reproduce the input bytes.
    fn run(&mut self, table: Params, depth: usize) -> Option<(Save, Params)> {
        if depth > MAX_DEPTH || self.attempts == 0 || Instant::now() >= self.deadline {
            return None;
        }

        let strict_offset = match parse_save(self.input, types_from(&table)) {
            Ok(save) => {
                if depth == 0 || write_save(&save).is_ok_and(|bytes| bytes == self.input) {
                    return Some((save, table));
                }
                return None;
            }
            Err(error) => error.offset().unwrap_or(0),
        };

        let groups = parse_save_lenient(self.input, types_from(&table))
            .ok()
            .map(|save| unresolved(&save))
            .unwrap_or_default();

        if let Some(group) = groups.first() {
            return self.try_group(table, group, depth);
        }
        self.try_scanned(table, strict_offset, depth)
    }

    fn try_group(&mut self, table: Params, group: &Group, depth: usize) -> Option<(Save, Params)> {
        for component in &group.components {
            for name in CANDIDATES {
                if self.attempts == 0 || Instant::now() >= self.deadline {
                    return None;
                }
                let candidate = StructType::from(*name);
                if candidate == component.current {
                    continue;
                }
                let mut next = table.clone();
                next.set(component.path.clone(), *name);

                self.attempts -= 1;
                let Some(after_save) = parse_save_lenient(self.input, types_from(&next)).ok()
                else {
                    continue;
                };
                let after = unresolved(&after_save);
                let after_component = after
                    .iter()
                    .find(|other| other.path == group.path)
                    .and_then(|other| {
                        other
                            .components
                            .iter()
                            .find(|other| other.path == component.path)
                    });
                let moved = match after_component {
                    None => true,
                    Some(other) => other.current != component.current,
                };
                if !moved {
                    continue;
                }

                if let Some(found) = self.run(next, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn try_scanned(
        &mut self,
        table: Params,
        error_offset: u64,
        depth: usize,
    ) -> Option<(Save, Params)> {
        let offset = usize::try_from(error_offset).unwrap_or(usize::MAX);
        for path in scanned_paths(self.input, offset) {
            for name in CANDIDATES {
                if self.attempts == 0 || Instant::now() >= self.deadline {
                    return None;
                }
                if matches!(table.get(&path), Some(ParamValue::Str(current)) if current == name) {
                    continue;
                }
                let mut next = table.clone();
                next.set(path.clone(), *name);
                if let Some(found) = self.retry(next, depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }
}

/// Parses `input`, inferring missing struct type hints when needed.
///
/// Returns the parsed save and the complete hint table (user hints plus the
/// ones found), or a [`GVAS_PARSE`] error with one plain sentence when no
/// candidate set gives an exact round trip.
pub(super) fn read_with_hints(input: &[u8], base: &Params) -> Result<(Save, Params)> {
    if let Ok(save) = parse_save(input, types_from(base)) {
        return Ok((save, base.clone()));
    }

    let mut search = Search {
        input,
        attempts: MAX_ATTEMPTS,
        deadline: Instant::now() + MAX_TIME,
    };
    if let Some(found) = search.run(base.clone(), 0) {
        return Ok(found);
    }

    match parse_save(input, types_from(base)) {
        Err(source) => {
            let offset = source.offset();
            let mut error = Error::new(
                GVAS_PARSE,
                format!(
                    "{source}. SavePrism could not infer the struct type hints this file needs."
                ),
            );
            if let Some(offset) = offset {
                error = error.with_offset(offset);
            }
            Err(error.with_source(source))
        }
        Ok(_) => Err(Error::new(
            GVAS_PARSE,
            "SavePrism could not infer the struct type hints this file needs.",
        )),
    }
}

/// Guards the search from a panic inside `uesave` (untrusted input).
pub(super) fn guarded_read(input: &[u8], base: &Params) -> Result<(Save, Params)> {
    guarded(GVAS_PARSE, "reading the save", || {
        read_with_hints(input, base)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNRESOLVABLE: &[u8] =
        include_bytes!("../../../../fixtures/synthetic/gvas-hints/map-unresolvable.sav");

    #[test]
    fn the_attempt_budget_stops_the_search() {
        let mut search = Search {
            input: UNRESOLVABLE,
            attempts: 1,
            deadline: Instant::now() + Duration::from_secs(10),
        };
        assert!(search.run(Params::new(), 0).is_none());
        assert_eq!(search.attempts, 0);
    }

    #[test]
    fn the_time_budget_stops_the_search() {
        let mut search = Search {
            input: UNRESOLVABLE,
            attempts: MAX_ATTEMPTS,
            deadline: Instant::now(),
        };
        assert!(search.run(Params::new(), 0).is_none());
        assert_eq!(search.attempts, MAX_ATTEMPTS);
    }
}
