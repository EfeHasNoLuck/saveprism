//! Tests of the Satisfactory codec.

use std::collections::BTreeMap;
use std::path::PathBuf;

use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Edit, Limits, NodeFlags, OwnedValue, Path, SaveDoc,
    Value, apply_edits,
};

use super::*;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn fixture() -> Vec<u8> {
    std::fs::read(fixtures().join("synthetic/satisfactory/invented.sav")).unwrap()
}

fn decode_with(bytes: &[u8], limits: &Limits) -> saveprism_core::Result<Decoded> {
    let mut ctx = DecodeCtx::new(limits);
    SatisfactoryCodec.decode(bytes, &mut ctx)
}

fn decode(bytes: &[u8]) -> Decoded {
    decode_with(bytes, &Limits::default()).unwrap()
}

fn encode(decoded: &Decoded) -> Vec<u8> {
    SatisfactoryCodec
        .encode(&decoded.doc, &decoded.params)
        .unwrap()
}

fn pointer(text: &str) -> Path {
    Path::parse(&format!("/{}", text.trim_start_matches('/'))).unwrap()
}

fn at<'a>(doc: &'a SaveDoc, path: &str) -> &'a saveprism_core::Node {
    doc.resolve(&pointer(path))
        .and_then(|id| doc.get(id))
        .unwrap_or_else(|| panic!("no node at {path}"))
}

fn int_at(doc: &SaveDoc, path: &str) -> i128 {
    match at(doc, path).value {
        Value::Int { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn float_at(doc: &SaveDoc, path: &str) -> f64 {
    match at(doc, path).value {
        Value::Float { v, .. } => v,
        ref other => panic!("{path} is {other:?}"),
    }
}

fn text_at(doc: &SaveDoc, path: &str) -> String {
    match &at(doc, path).value {
        Value::Str(text) => text.clone(),
        other => panic!("{path} is {other:?}"),
    }
}

fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) -> saveprism_core::Result<()> {
    apply_edits(
        &mut decoded.doc,
        &[Edit::Set {
            path: pointer(path),
            value,
        }],
        &Limits::default(),
    )
}

fn hinted(decoded: &Decoded, path: &str) -> String {
    at(&decoded.doc, path)
        .type_hint
        .as_ref()
        .map(|hint| hint.as_str().to_string())
        .unwrap_or_default()
}

/// Every leaf below `id`, together with its `/`-separated key path.
fn walk(doc: &SaveDoc, id: saveprism_core::NodeId, prefix: &str, out: &mut Vec<(String, String)>) {
    let Some(node) = doc.get(id) else {
        return;
    };
    match &node.value {
        Value::Object(entries) => {
            for (key, child) in entries {
                walk(doc, *child, &format!("{prefix}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                walk(doc, *child, &format!("{prefix}/{index}"), out);
            }
        }
        Value::Map(entries) => {
            for (index, (_, child)) in entries.iter().enumerate() {
                walk(doc, *child, &format!("{prefix}/{index}"), out);
            }
        }
        Value::Ref(child) => walk(doc, *child, prefix, out),
        _ => {
            let hint = node
                .type_hint
                .as_ref()
                .map(|hint| hint.as_str().to_string())
                .unwrap_or_default();
            out.push((prefix.to_string(), hint));
        }
    }
}

#[test]
fn header_fields_are_a_tree_with_editable_numbers() {
    let decoded = decode(&fixture());
    assert_eq!(int_at(&decoded.doc, "Header/HeaderVersion"), 14);
    assert_eq!(int_at(&decoded.doc, "Header/SaveVersion"), 52);
    assert_eq!(int_at(&decoded.doc, "Header/BuildVersion"), 424353);
    assert_eq!(text_at(&decoded.doc, "Header/SaveName"), "Invented Save");
    assert_eq!(text_at(&decoded.doc, "Header/MapName"), "Persistent_Level");
    assert_eq!(int_at(&decoded.doc, "Header/PlayedSeconds"), 3600);
    assert_eq!(hinted(&decoded, "Header/BuildVersion"), "SAT:UInt32");
    assert!(
        at(&decoded.doc, "Header/SaveName")
            .flags
            .contains(NodeFlags::READ_ONLY)
    );
}

#[test]
fn body_has_levels_classes_properties_and_inventory_stacks() {
    let decoded = decode(&fixture());
    let doc = &decoded.doc;
    assert_eq!(int_at(doc, "Body/LevelCount"), 1);
    assert_eq!(int_at(doc, "Body/ObjectCount"), 2);
    assert_eq!(text_at(doc, "Body/Levels/0/Name"), "SubLevel");
    assert_eq!(text_at(doc, "Body/Levels/1/Name"), "Persistent_Level");
    assert_eq!(
        int_at(
            doc,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mGold"
        ),
        500
    );
    assert_eq!(
        hinted(
            &decoded,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mGold"
        ),
        "SAT:IntProperty"
    );
    let stacks = "Body/Levels/1/Classes/InventedActor_C/0/Properties/mInventoryStacks";
    assert_eq!(int_at(doc, &format!("{stacks}/0/NumItems")), 3);
    assert_eq!(int_at(doc, &format!("{stacks}/1/NumItems")), 42);
    assert_eq!(
        hinted(&decoded, &format!("{stacks}/1/NumItems")),
        "SAT:IntProperty"
    );
    assert_eq!(
        float_at(
            doc,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mHealth"
        ),
        100.0
    );
    assert!(matches!(
        at(
            doc,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mFlag"
        )
        .value,
        Value::Bool(true)
    ));

    let mut leaves = Vec::new();
    walk(doc, doc.root(), "", &mut leaves);
    assert!(leaves.iter().any(|(path, _)| path.ends_with("/NumItems")));
}

#[test]
fn unchanged_save_round_trips_byte_for_byte() {
    let bytes = fixture();
    let decoded = decode(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn inventory_count_and_scalars_are_editable_in_place() {
    let bytes = fixture();
    let mut decoded = decode(&bytes);
    let stack = "Body/Levels/1/Classes/InventedActor_C/0/Properties/mInventoryStacks/1/NumItems";
    set(&mut decoded, stack, OwnedValue::Int(9)).unwrap();
    set(
        &mut decoded,
        "Body/Levels/1/Classes/InventedActor_C/0/Properties/mGold",
        OwnedValue::Int(501),
    )
    .unwrap();
    set(
        &mut decoded,
        "Body/Levels/1/Classes/InventedActor_C/0/Properties/mHealth",
        OwnedValue::Float(55.5),
    )
    .unwrap();
    set(&mut decoded, "Header/BuildVersion", OwnedValue::Int(424354)).unwrap();

    let written = encode(&decoded);
    assert_ne!(written, bytes);
    let again = decode(&written);
    assert_eq!(int_at(&again.doc, stack), 9);
    assert_eq!(
        int_at(
            &again.doc,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mGold"
        ),
        501
    );
    assert_eq!(
        float_at(
            &again.doc,
            "Body/Levels/1/Classes/InventedActor_C/0/Properties/mHealth"
        ),
        55.5
    );
    assert_eq!(int_at(&again.doc, "Header/BuildVersion"), 424354);
    assert_eq!(int_at(&again.doc, "Body/LevelCount"), 1);
}

#[test]
fn strings_are_read_only() {
    let mut decoded = decode(&fixture());
    let error = set(
        &mut decoded,
        "Body/Levels/1/Classes/InventedActor_C/0/Properties/mName",
        OwnedValue::Str("changed".to_string()),
    )
    .unwrap_err();
    assert_eq!(error.code(), saveprism_core::codes::CORE_READ_ONLY);
}

#[test]
fn detection_is_by_content_and_does_not_claim_gvas() {
    let bytes = fixture();
    let ctx = saveprism_core::SniffCtx::default();
    assert_eq!(SatisfactoryCodec.sniff(&bytes, &ctx), Confidence::HIGH);
    let gvas = b"GVAS\x00\x00\x00\x00";
    assert_eq!(SatisfactoryCodec.sniff(gvas, &ctx), Confidence::NONE);
    let mut wrong = bytes.clone();
    if let Some(slot) = wrong.get_mut(0..4) {
        slot.copy_from_slice(&99_u32.to_le_bytes());
    }
    assert_eq!(SatisfactoryCodec.sniff(&wrong, &ctx), Confidence::NONE);
}

#[test]
fn a_chunk_outside_the_output_budget_is_refused() {
    let bytes = fixture();
    let limits = Limits {
        ratio_floor_bytes: 0,
        max_ratio: 1,
        ..Limits::default()
    };
    let error = decode_with(&bytes, &limits).unwrap_err();
    assert_eq!(error.code(), saveprism_core::codes::CORE_LIMIT_DECODED);
}

fn collect_real_saves(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_real_saves(&path, out);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("sav"))
        {
            out.push(path);
        }
    }
}

fn leaf_values(doc: &SaveDoc) -> BTreeMap<String, Value> {
    let mut leaves = Vec::new();
    walk(doc, doc.root(), "", &mut leaves);
    leaves
        .into_iter()
        .filter_map(|(path, _)| doc.resolve(&pointer(&path)).map(|id| (path, id)))
        .filter_map(|(path, id)| doc.get(id).map(|node| (path, node.value.clone())))
        .collect()
}

#[test]
#[ignore = "reads the local, git-ignored fixtures/external/satisfactory saves"]
fn real_saves_round_trip_and_edit_an_inventory_count() {
    let root = fixtures().join("external/satisfactory");
    let mut saves = Vec::new();
    collect_real_saves(&root, &mut saves);
    if saves.is_empty() {
        eprintln!("no real Satisfactory saves under {}", root.display());
        return;
    }

    for path in saves {
        let bytes = std::fs::read(&path).unwrap();
        let started = std::time::Instant::now();
        let mut decoded = decode(&bytes);
        let elapsed = started.elapsed();
        assert_eq!(
            encode(&decoded),
            bytes,
            "{} does not round-trip unchanged",
            path.display()
        );

        let mut leaves = Vec::new();
        walk(&decoded.doc, decoded.doc.root(), "", &mut leaves);
        let mut editable = Vec::new();
        for (leaf_path, _) in &leaves {
            let Some(id) = decoded.doc.resolve(&pointer(leaf_path)) else {
                continue;
            };
            let Some(node) = decoded.doc.get(id) else {
                continue;
            };
            if !node.flags.contains(NodeFlags::READ_ONLY) && node.span.is_some() {
                editable.push((leaf_path.clone(), node.value.clone()));
            }
        }

        let num_items = editable
            .iter()
            .find(|(leaf, _)| leaf.ends_with("/NumItems"))
            .map(|(leaf, value)| (leaf.clone(), value.clone()));
        let scalar = editable
            .iter()
            .find(|(leaf, value)| {
                !leaf.ends_with("/NumItems")
                    && leaf.contains("/Properties/")
                    && matches!(
                        value,
                        Value::Int {
                            width: saveprism_core::IntWidth::I32,
                            ..
                        }
                    )
            })
            .map(|(leaf, value)| (leaf.clone(), value.clone()));

        let Some((num_items_path, num_items_value)) = num_items else {
            eprintln!("{} has no editable NumItems", path.display());
            continue;
        };
        let Value::Int { v: old_count, .. } = num_items_value else {
            continue;
        };
        let new_count = old_count.saturating_add(1);
        set(&mut decoded, &num_items_path, OwnedValue::Int(new_count)).unwrap();

        let mut edited_paths = vec![num_items_path.clone()];
        if let Some((scalar_path, Value::Int { v: old, .. })) = scalar {
            set(
                &mut decoded,
                &scalar_path,
                OwnedValue::Int(old.saturating_add(1)),
            )
            .unwrap();
            edited_paths.push(scalar_path);
        }

        let written = encode(&decoded);
        let again = decode(&written);
        assert_eq!(int_at(&again.doc, &num_items_path), new_count);

        let before = leaf_values(&decoded.doc);
        let after = leaf_values(&again.doc);
        assert_eq!(
            before.len(),
            after.len(),
            "{} changed its leaf count",
            path.display()
        );
        for (leaf, value) in &before {
            if edited_paths.iter().any(|edited| edited == leaf) {
                continue;
            }
            assert_eq!(
                after.get(leaf),
                Some(value),
                "{} changed {leaf}",
                path.display()
            );
        }

        let objects = int_at(&again.doc, "Body/ObjectCount");
        let levels = int_at(&again.doc, "Body/LevelCount");
        let properties = leaves
            .iter()
            .filter(|(leaf, _)| leaf.contains("/Properties/"))
            .count();
        eprintln!(
            "{}: {} bytes, {} levels, {} objects, {} property leaves, {} editable leaves, decode {:?}",
            path.display(),
            bytes.len(),
            levels,
            objects,
            properties,
            leaves.len(),
            elapsed
        );
    }
}
