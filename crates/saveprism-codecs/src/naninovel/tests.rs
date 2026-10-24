use saveprism_core::{
    Codec, DecodeCtx, Edit, Limits, OwnedValue, Path, SaveDoc, Value, apply_edits,
};

use super::*;

fn fixture() -> Vec<u8> {
    let state = r#"{"LocalVariables":[{"name":"coins","scope":0,"value":{"type":1,"stringValue":"","numericValue":50,"booleanValue":false}},{"name":"hero_name","scope":0,"value":{"type":0,"stringValue":"Aster","numericValue":0,"booleanValue":false}}],"GlobalVariables":[{"name":"g_cleared","scope":1,"value":{"type":2,"stringValue":"","numericValue":0,"booleanValue":false}}]}"#;
    let other = r#"{"items":{"potion":3},"capacity":20}"#;
    let json = serde_json::json!({
        "objectJsonMap": {
            "keys": [
                "Naninovel.CustomVariableManager+GameState, Elringus.Naninovel.Runtime",
                "Game.InventoryService+State, Assembly-CSharp",
            ],
            "values": [state, other],
        },
        "playbackSpot": {"scriptPath": "Chapter1", "lineIndex": 12, "inlineIndex": 0},
        "rollbackStackJson": "[]",
        "thumbnailBase64": "",
    });
    serde_json::to_vec(&json).unwrap()
}

fn doc(bytes: &[u8]) -> SaveDoc {
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    NaninovelCodec.decode(bytes, &mut ctx).unwrap().doc
}

fn encode(doc: &SaveDoc) -> Vec<u8> {
    NaninovelCodec.encode(doc, &Params::new()).unwrap()
}

#[test]
fn round_trip_is_exact() {
    let bytes = fixture();
    let decoded = doc(&bytes);
    assert_eq!(encode(&decoded), bytes);
}

#[test]
fn state_strings_are_inlined() {
    let decoded = doc(&fixture());
    let path = Path::parse("/objectJsonMap/values/0/LocalVariables/0/value/numericValue").unwrap();
    let id = decoded.resolve(&path).unwrap();
    assert_eq!(
        decoded.get(id).map(|node| &node.value),
        Some(&Value::Int {
            v: 50,
            width: saveprism_core::IntWidth::Unbounded
        })
    );
}

#[test]
fn edits_are_written_back_and_read_again() {
    let mut decoded = doc(&fixture());
    apply_edits(
        &mut decoded,
        &[
            Edit::Set {
                path: Path::parse("/objectJsonMap/values/0/LocalVariables/0/value/numericValue")
                    .unwrap(),
                value: OwnedValue::Int(900),
            },
            Edit::Set {
                path: Path::parse("/objectJsonMap/values/0/LocalVariables/1/value/stringValue")
                    .unwrap(),
                value: OwnedValue::Str("Nova".to_string()),
            },
        ],
        &Limits::default(),
    )
    .unwrap();
    let bytes = encode(&decoded);
    let outer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let state = outer["objectJsonMap"]["values"][0].as_str().unwrap();
    assert!(state.contains("\"numericValue\":900"));
    assert!(state.contains("\"stringValue\":\"Nova\""));
    // The untouched state keeps its exact original text.
    assert_eq!(
        outer["objectJsonMap"]["values"][1].as_str().unwrap(),
        r#"{"items":{"potion":3},"capacity":20}"#
    );

    let again = doc(&bytes);
    assert_eq!(
        again
            .resolve(
                &Path::parse("/objectJsonMap/values/0/LocalVariables/0/value/numericValue")
                    .unwrap()
            )
            .and_then(|id| again.get(id))
            .map(|node| &node.value),
        Some(&Value::Int {
            v: 900,
            width: saveprism_core::IntWidth::Unbounded
        })
    );
}

#[test]
fn a_bad_state_map_is_refused() {
    let bytes =
        br#"{"objectJsonMap":{"keys":["Naninovel.CustomVariableManager+GameState"],"values":[]}}"#;
    let limits = Limits::default();
    let mut ctx = DecodeCtx::new(&limits);
    let error = NaninovelCodec.decode(bytes, &mut ctx).unwrap_err();
    assert_eq!(error.code(), NANI_BAD_MAP);
}

#[test]
fn a_broken_state_string_stays_a_string() {
    let bytes = br#"{"objectJsonMap":{"keys":["Naninovel.CustomVariableManager+GameState"],"values":["{not json"]}}"#;
    let decoded = doc(bytes);
    assert_eq!(encode(&decoded), bytes.to_vec());
}
