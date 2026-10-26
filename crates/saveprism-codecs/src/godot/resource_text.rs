//! `format.godot-resource-text`: Godot text resources (`.tres`, `format=2` and `format=3`).
//!
//! Written from the public Godot text-resource format. A file starts with a `[gd_resource ...]`
//! header, followed by optional `[ext_resource ...]` and `[sub_resource ...]` sections and the main
//! `[resource]` section whose `key = <Variant text>` lines hold the saved values. The text is
//! parsed with the same Variant parser as the `ConfigFile` codec; nothing is evaluated.
//!
//! # Tree
//!
//! The root is an `Object` with the keys `gd_resource` (header attributes), `ext_resource`
//! (read-only array), `sub_resource` (array of `{ id, type, properties }`) and `resource`
//! (the main properties). Scalars inside `properties` are editable with the same rules as
//! `ConfigFile`: booleans, integers, floats and strings.
//!
//! # Read-only
//!
//! `ExtResource(...)` and `SubResource(...)` references, typed containers such as
//! `Array[Resource]([...])` or `Dictionary[String, int]({...})`, arrays and dictionaries stay
//! read-only, so an edit never changes the resource graph or a container's shape. Header
//! attributes and external-resource metadata are read-only too.
//!
//! # Exact writing
//!
//! The original text is kept as the codec parameter `original`; writing patches only the tokens an
//! edit changed and copies everything else byte for byte.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, Node, NodeFlags, NodeId, ParamValue, Params,
    Registry, Result, SniffCtx, TypeHint, Value,
};

use super::variant_text::{TextOptions, balanced, parse_value, write_patched};
use super::{GODOT_PARSE, GODOT_WRITE, PREFIX};

/// The `format.godot-resource-text` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GodotResourceTextCodec;

const TEXT: TextOptions = TextOptions {
    prefix: "GODOT:tres:",
    locked_constructors: &["ExtResource", "SubResource"],
    typed_containers: true,
};

/// Registers the text-resource codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GodotResourceTextCodec));
}

impl Codec for GodotResourceTextCodec {
    fn id(&self) -> &'static str {
        "format.godot-resource-text"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        let header = text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("[gd_resource")
        });
        if !header {
            return Confidence::NONE;
        }
        let named = ctx
            .file_name
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".tres"));
        Confidence::new(if named { 0.97 } else { 0.85 })
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let text = std::str::from_utf8(input)
            .map_err(|_| Error::new(GODOT_PARSE, "the Godot text resource is not UTF-8"))?;
        let mut doc = ctx.new_doc();
        let mut current: Option<Section> = None;
        let mut gd_resource = None;
        let mut ext_resources = Vec::new();
        let mut sub_resources = Vec::new();
        let mut resource = None;
        let mut version = 3i64;
        let mut offset = 0usize;
        let mut lines = text.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            let mut consumed = line.len();
            let clean = line.trim();
            if clean.is_empty() || clean.starts_with(';') {
                offset += consumed;
                continue;
            }
            if let Some((name, attrs)) = parse_section(clean)? {
                if let Some(previous) = current.take() {
                    finish_section(
                        previous,
                        &mut doc,
                        &mut gd_resource,
                        &mut ext_resources,
                        &mut sub_resources,
                        &mut resource,
                        &mut version,
                    )?;
                }
                current = Some(match name.as_str() {
                    "gd_resource" => Section::Header(parse_attrs(attrs)?),
                    "ext_resource" => Section::External(parse_attrs(attrs)?),
                    "sub_resource" => Section::SubResource {
                        attrs: parse_attrs(attrs)?,
                        properties: Vec::new(),
                    },
                    "resource" => Section::Resource {
                        properties: Vec::new(),
                    },
                    _ => Section::Ignored,
                });
                offset += consumed;
                continue;
            }
            let Some(eq) = line.find('=') else {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("expected a section or key = value line at byte {offset}"),
                ));
            };
            let key = line.get(..eq).unwrap_or_default().trim().to_owned();
            if key.is_empty() {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("empty property name at byte {offset}"),
                ));
            }
            let mut value = line.get(eq + 1..).unwrap_or_default().to_owned();
            let mut value_offset = offset + eq + 1;
            while !balanced(&value) {
                let Some(next) = lines.next() else {
                    return Err(Error::new(
                        GODOT_PARSE,
                        "unterminated multi-line Godot resource value",
                    ));
                };
                consumed += next.len();
                value.push_str(next);
            }
            let leading = value.len() - value.trim_start().len();
            value_offset += leading;
            let token = value.trim();
            let keep = matches!(
                current,
                Some(Section::SubResource { .. } | Section::Resource { .. })
            );
            if keep {
                let id = parse_value(token, value_offset, &mut doc, ctx.limits().max_depth, TEXT)?;
                match current.as_mut() {
                    Some(Section::SubResource { properties, .. }) => properties.push((key, id)),
                    Some(Section::Resource { properties }) => properties.push((key, id)),
                    _ => {}
                }
            }
            offset += consumed;
        }
        if let Some(previous) = current.take() {
            finish_section(
                previous,
                &mut doc,
                &mut gd_resource,
                &mut ext_resources,
                &mut sub_resources,
                &mut resource,
                &mut version,
            )?;
        }
        let Some(header) = gd_resource else {
            return Err(Error::new(
                GODOT_PARSE,
                "the file has no [gd_resource] header",
            ));
        };
        let mut root_entries: Vec<(String, NodeId)> = vec![("gd_resource".to_owned(), header)];
        if !ext_resources.is_empty() {
            let array = push_read_only(&mut doc, Value::Array(ext_resources), "ext_resource-list")?;
            root_entries.push(("ext_resource".to_owned(), array));
        }
        if !sub_resources.is_empty() {
            let array = push_read_only(&mut doc, Value::Array(sub_resources), "sub_resource-list")?;
            root_entries.push(("sub_resource".to_owned(), array));
        }
        if let Some(resource) = resource {
            root_entries.push(("resource".to_owned(), resource));
        }
        let mut root = Node::new(Value::Object(root_entries));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}tres:root")));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        params.set("version", version);
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &saveprism_core::SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(
                GODOT_WRITE,
                "the original Godot text resource is missing",
            ));
        };
        write_patched(original, doc, TEXT.prefix)
    }
}

enum Section {
    Header(Vec<(String, String)>),
    External(Vec<(String, String)>),
    SubResource {
        attrs: Vec<(String, String)>,
        properties: Vec<(String, NodeId)>,
    },
    Resource {
        properties: Vec<(String, NodeId)>,
    },
    Ignored,
}

#[allow(clippy::too_many_arguments)]
fn finish_section(
    section: Section,
    doc: &mut saveprism_core::SaveDoc,
    gd_resource: &mut Option<NodeId>,
    ext_resources: &mut Vec<NodeId>,
    sub_resources: &mut Vec<NodeId>,
    resource: &mut Option<NodeId>,
    version: &mut i64,
) -> Result<()> {
    match section {
        Section::Header(attrs) => {
            if let Some(format) = attrs
                .iter()
                .find(|(key, _)| key == "format")
                .and_then(|(_, value)| value.parse::<i64>().ok())
            {
                *version = if format <= 2 { 3 } else { 4 };
            }
            let entries = push_attributes(doc, &attrs)?;
            *gd_resource = Some(entries);
        }
        Section::External(attrs) => {
            ext_resources.push(push_attributes(doc, &attrs)?);
        }
        Section::SubResource { attrs, properties } => {
            let properties = push_properties(doc, properties)?;
            let mut entries = vec![("properties".to_owned(), properties)];
            for (key, value) in &attrs {
                if key == "id" || key == "type" {
                    entries.insert(
                        0,
                        (
                            key.clone(),
                            push_read_only(doc, Value::Str(value.clone()), "header")?,
                        ),
                    );
                }
            }
            let wrapper = push_read_only(doc, Value::Object(entries), "sub_resource")?;
            sub_resources.push(wrapper);
        }
        Section::Resource { properties } => {
            *resource = Some(push_properties(doc, properties)?);
        }
        Section::Ignored => {}
    }
    Ok(())
}

/// Builds one read-only object holding string attributes.
fn push_attributes(
    doc: &mut saveprism_core::SaveDoc,
    attrs: &[(String, String)],
) -> Result<NodeId> {
    let mut entries = Vec::with_capacity(attrs.len());
    for (key, value) in attrs {
        let id = push_read_only(doc, Value::Str(value.clone()), "header")?;
        entries.push((key.clone(), id));
    }
    push_read_only(doc, Value::Object(entries), "header-object")
}

fn push_properties(
    doc: &mut saveprism_core::SaveDoc,
    properties: Vec<(String, NodeId)>,
) -> Result<NodeId> {
    let mut node = Node::new(Value::Object(properties));
    node.type_hint = Some(TypeHint::new(format!("{PREFIX}tres:properties")));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

fn push_read_only(doc: &mut saveprism_core::SaveDoc, value: Value, hint: &str) -> Result<NodeId> {
    let mut node = Node::new(value);
    node.type_hint = Some(TypeHint::new(format!("{PREFIX}tres:{hint}")));
    node.flags.insert(NodeFlags::READ_ONLY);
    doc.push(node)
}

/// Splits `[name attr=value ...]` into the name and the attribute text.
fn parse_section(line: &str) -> Result<Option<(String, &str)>> {
    let Some(rest) = line.strip_prefix('[') else {
        return Ok(None);
    };
    let name_end = rest
        .find(|character: char| character.is_ascii_whitespace() || character == ']')
        .unwrap_or(rest.len());
    let name = rest.get(..name_end).unwrap_or_default();
    if name.is_empty() {
        return Err(Error::new(GODOT_PARSE, "empty section name"));
    }
    let tail = rest.get(name_end..).unwrap_or_default();
    let Some(close) = find_close(tail) else {
        return Err(Error::new(
            GODOT_PARSE,
            "section header has no closing bracket",
        ));
    };
    let attrs = tail.get(..close).unwrap_or_default();
    let after = tail.get(close + 1..).unwrap_or_default().trim();
    if !after.is_empty() && !after.starts_with(';') {
        return Err(Error::new(
            GODOT_PARSE,
            "unexpected text after a section header",
        ));
    }
    Ok(Some((name.to_owned(), attrs)))
}

/// Index of the `]` that closes a section header, ignoring quoted text.
fn find_close(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    let mut quote = false;
    let mut escaped = false;
    while let Some(byte) = bytes.get(at) {
        if quote {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quote = false;
            }
            at += 1;
            continue;
        }
        match byte {
            b'"' => quote = true,
            b']' => return Some(at),
            _ => {}
        }
        at += 1;
    }
    None
}

/// Parses `key=value` attribute pairs; quoted values may contain spaces.
fn parse_attrs(text: &str) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let bytes = text.as_bytes();
    loop {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if at >= text.len() {
            break;
        }
        let key_start = at;
        while bytes
            .get(at)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && *byte != b'=')
        {
            at += 1;
        }
        let key = text.get(key_start..at).unwrap_or_default().to_owned();
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'=') {
            return Err(Error::new(GODOT_PARSE, "section attribute lacks an '='"));
        }
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        let value = if bytes.get(at) == Some(&b'"') {
            read_quoted(text, &mut at)?
        } else {
            let start = at;
            while bytes
                .get(at)
                .is_some_and(|byte| !byte.is_ascii_whitespace())
            {
                at += 1;
            }
            text.get(start..at).unwrap_or_default().to_owned()
        };
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    Ok(out)
}

/// Reads a quoted attribute value and decodes its escapes.
fn read_quoted(text: &str, at: &mut usize) -> Result<String> {
    let bytes = text.as_bytes();
    *at += 1;
    let mut out = String::new();
    loop {
        let Some(byte) = bytes.get(*at).copied() else {
            return Err(Error::new(GODOT_PARSE, "unterminated quoted attribute"));
        };
        match byte {
            b'"' => {
                *at += 1;
                return Ok(out);
            }
            b'\\' => {
                *at += 1;
                let Some(escape) = bytes.get(*at).copied() else {
                    return Err(Error::new(GODOT_PARSE, "unterminated attribute escape"));
                };
                *at += 1;
                match escape {
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    other => out.push(char::from(other)),
                }
            }
            _ => {
                let character = text
                    .get(*at..)
                    .and_then(|rest| rest.chars().next())
                    .ok_or_else(|| Error::new(GODOT_PARSE, "invalid attribute text"))?;
                out.push(character);
                *at += character.len_utf8();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use saveprism_core::{Edit, Limits, OwnedValue, Path};

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/godot")
            .join(name);
        std::fs::read(path).expect("fixture exists")
    }

    fn decode(bytes: &[u8]) -> Decoded {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        GodotResourceTextCodec
            .decode(bytes, &mut ctx)
            .expect("text resource parses")
    }

    fn encode(decoded: &Decoded) -> Vec<u8> {
        GodotResourceTextCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("text resource writes")
    }

    fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
        saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value,
            }],
            &Limits::default(),
        )
    }

    #[test]
    fn sniff_separates_text_resources_from_config_files() {
        let tres = fixture("resource4.tres");
        let named = SniffCtx {
            file_name: Some("player.tres"),
            depth: 0,
        };
        assert!(GodotResourceTextCodec.sniff(&tres, &named) > Confidence::new(0.9));
        let unnamed = SniffCtx {
            file_name: None,
            depth: 0,
        };
        assert!(GodotResourceTextCodec.sniff(&tres, &unnamed) > Confidence::NONE);
        let config = SniffCtx {
            file_name: Some("settings.cfg"),
            depth: 0,
        };
        assert_eq!(
            GodotResourceTextCodec.sniff(b"[Player]\ngold=1\n", &config),
            Confidence::NONE
        );
        assert_eq!(
            super::super::config::GodotConfigCodec.sniff(&tres, &named),
            Confidence::NONE
        );
    }

    #[test]
    fn generated_text_resources_round_trip_exactly() {
        for (name, version) in [("resource4.tres", 4), ("resource3.tres", 3)] {
            let bytes = fixture(name);
            let decoded = decode(&bytes);
            assert_eq!(encode(&decoded), bytes, "{name}");
            assert_eq!(
                decoded.params.get("version"),
                Some(&ParamValue::Int(version)),
                "{name}"
            );
        }
    }

    #[test]
    fn edits_scalars_without_touching_the_rest() {
        let bytes = fixture("resource4.tres");
        let mut decoded = decode(&bytes);
        set(&mut decoded, "/resource/level", OwnedValue::Int(99)).expect("level edits");
        set(
            &mut decoded,
            "/resource/name",
            OwnedValue::Str("Mira 南".into()),
        )
        .expect("name edits");
        set(
            &mut decoded,
            "/sub_resource/0/properties/gold",
            OwnedValue::Int(777),
        )
        .expect("subresource edits");
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("level = 99"));
        assert!(text.contains("name = \"Mira 南\""));
        assert!(text.contains("gold = 777"));
        assert!(text.contains("uid://invented1"));
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn references_and_typed_containers_stay_read_only() {
        let bytes = fixture("resource4.tres");
        let mut decoded = decode(&bytes);
        assert!(set(&mut decoded, "/resource/stats", OwnedValue::Int(1)).is_err());
        assert!(
            set(
                &mut decoded,
                "/resource/script/0",
                OwnedValue::Str("x".into())
            )
            .is_err()
        );
        assert!(
            set(
                &mut decoded,
                "/resource/tags/0",
                OwnedValue::Str("x".into())
            )
            .is_err()
        );
        assert!(set(&mut decoded, "/resource/colors/0", OwnedValue::Int(2)).is_err());
        assert!(set(&mut decoded, "/resource/inventory/1", OwnedValue::Int(9)).is_ok());
    }

    #[test]
    fn godot3_references_are_locked_too() {
        let bytes = fixture("resource3.tres");
        let mut decoded = decode(&bytes);
        assert!(set(&mut decoded, "/resource/script", OwnedValue::Int(1)).is_err());
        assert!(set(&mut decoded, "/resource/stats", OwnedValue::Int(1)).is_err());
        set(&mut decoded, "/resource/level", OwnedValue::Int(21)).expect("godot3 level edits");
        let output = encode(&decoded);
        assert!(
            std::str::from_utf8(&output)
                .expect("utf8")
                .contains("level = 21")
        );
    }

    use proptest::prelude::*;

    fn writes_back(bytes: &[u8]) {
        if let Ok(decoded) =
            GodotResourceTextCodec.decode(bytes, &mut DecodeCtx::new(&Limits::default()))
        {
            let written = GodotResourceTextCodec
                .encode(&decoded.doc, &decoded.params)
                .expect("decoded text resource writes");
            let again = GodotResourceTextCodec
                .decode(&written, &mut DecodeCtx::new(&Limits::default()))
                .expect("written text resource decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
            writes_back(&body);
        }
    }
}
