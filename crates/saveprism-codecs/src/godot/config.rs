//! `format.godot-config`: Godot `ConfigFile` text (`.cfg`, `.ini`, `.save`).
//!
//! Written from the public `ConfigFile` documentation and Godot's Variant text syntax. The text is
//! parsed into a data tree; nothing in the file is evaluated.
//!
//! # Tree
//!
//! The root is an `Object` of sections. Its keys are the section names (the unnamed leading
//! section is `""`), and every value is an `Object` of the keys in that section. Empty sections
//! are not represented; comments and blank lines are kept only as bytes of the original text.
//!
//! | ConfigFile value | Node | Hint |
//! |---|---|---|
//! | `true` / `false` | `Bool` | `GODOT:config:bool` |
//! | integer | `Int` (unbounded width, as text has none) | `GODOT:config:int` |
//! | float (`1e10`, `inf`, `-inf`, `nan`) | `Float` (F64) | `GODOT:config:float` |
//! | `"string"` | `Str` | `GODOT:config:string` |
//! | `null` | `Null` (read-only) | `GODOT:config:null` |
//! | `[...]` | `Array` (read-only) | `GODOT:config:array` |
//! | `{...}` | `Map` (read-only; keys stay nodes) | `GODOT:config:dict` |
//! | `Vector2(...)`, `Color(...)`, ... | `Array` of arguments (read-only) | `GODOT:config:constructor:<name>` |
//! | `&"name"` | `Str` (read-only) | `GODOT:config:constructor:StringName` |
//!
//! Editable: booleans, integers, floats and strings, including values inside arrays and
//! constructors. Containers stay read-only, so an edit never changes the shape of the file.
//!
//! # Exact writing
//!
//! The original text is kept as the codec parameter `original`. Every editable node records the
//! byte span of its token; writing replaces only the tokens an edit changed, in Godot's own
//! number and string syntax, and copies every comment, blank line and spacing byte unchanged.

use std::sync::Arc;

use saveprism_core::limits::len_u64;
use saveprism_core::{
    Codec, Confidence, DecodeCtx, Decoded, Error, Node, NodeFlags, NodeId, ParamValue, Params,
    Registry, Result, SaveDoc, SniffCtx, TypeHint, Value,
};

use super::variant_text::{TextOptions, balanced, parse_value, write_patched};
use super::{GODOT_PARSE, GODOT_WRITE, PREFIX};

const TEXT: TextOptions = TextOptions {
    prefix: "GODOT:config:",
    locked_constructors: &[],
    typed_containers: false,
};

/// Registers the `format.godot-config` codec.
pub fn register(reg: &mut Registry) {
    reg.add_codec(Arc::new(GodotConfigCodec));
}

/// The `format.godot-config` codec.
#[derive(Clone, Copy, Debug, Default)]
pub struct GodotConfigCodec;

impl Codec for GodotConfigCodec {
    fn id(&self) -> &'static str {
        "format.godot-config"
    }

    fn sniff(&self, input: &[u8], ctx: &SniffCtx<'_>) -> Confidence {
        let Ok(text) = std::str::from_utf8(input) else {
            return Confidence::NONE;
        };
        if text.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("[gd_resource") || line.starts_with("[node ")
        }) {
            // Godot text resources and scenes are not ConfigFiles.
            return Confidence::NONE;
        }
        let named = ctx.file_name.is_some_and(|name| {
            let lower = name.to_ascii_lowercase();
            [".cfg", ".ini", ".save"]
                .iter()
                .any(|extension| lower.ends_with(extension))
        });
        let has_section = text.lines().any(|line| {
            let line = line.trim();
            line.starts_with('[')
                && line.find(']').is_some_and(|close| {
                    line.get(close + 1..).is_some_and(|tail| {
                        let tail = tail.trim();
                        tail.is_empty() || tail.starts_with(';')
                    })
                })
        });
        let has_assignment = text.lines().any(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('[') {
                return false;
            }
            line.split_once('=').is_some_and(|(key, _)| {
                let key = key.trim();
                !key.is_empty() && !key.contains([':', '"', '{', '}'])
            })
        });
        if has_section && has_assignment {
            if named {
                Confidence::new(0.9)
            } else {
                Confidence::new(0.8)
            }
        } else if named && has_assignment {
            Confidence::new(0.55)
        } else {
            Confidence::NONE
        }
    }

    fn decode(&self, input: &[u8], ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
        ctx.limits().check_input(len_u64(input.len()))?;
        let text = std::str::from_utf8(input)
            .map_err(|_| Error::new(GODOT_PARSE, "Godot ConfigFile is not UTF-8"))?;
        let mut doc = ctx.new_doc();
        let mut sections: Vec<(String, NodeId)> = Vec::new();
        let mut section = String::new();
        let mut keys: Vec<(String, NodeId)> = Vec::new();
        let mut offset = 0usize;
        let mut lines = text.split_inclusive('\n').peekable();
        while let Some(line) = lines.next() {
            let mut consumed = line.len();
            let clean = line.trim();
            if clean.is_empty() || clean.starts_with(';') {
                offset += consumed;
                continue;
            }
            if let Some(rest) = clean.strip_prefix('[')
                && let Some(close) = rest.find(']')
            {
                let tail = rest.get(close + 1..).unwrap_or_default().trim();
                if tail.is_empty() || tail.starts_with(';') {
                    if !keys.is_empty() {
                        let mut node = Node::new(Value::Object(std::mem::take(&mut keys)));
                        node.type_hint = Some(TypeHint::new(format!("{PREFIX}config:section")));
                        node.flags.insert(NodeFlags::READ_ONLY);
                        let id = doc.push(node)?;
                        sections.push((section.clone(), id));
                    }
                    section = rest.get(..close).unwrap_or_default().trim().to_owned();
                    offset += consumed;
                    continue;
                }
            }
            let Some(eq) = line.find('=') else {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("expected section or key=value at byte {offset}"),
                ));
            };
            let key = line.get(..eq).unwrap_or_default().trim().to_owned();
            if key.is_empty() {
                return Err(Error::new(
                    GODOT_PARSE,
                    format!("empty key at byte {offset}"),
                ));
            }
            let mut value = line.get(eq + 1..).unwrap_or_default().to_owned();
            let mut value_offset = offset + eq + 1;
            while !balanced(&value) {
                let Some(next) = lines.next() else {
                    return Err(Error::new(
                        GODOT_PARSE,
                        "unterminated multi-line Godot value",
                    ));
                };
                consumed += next.len();
                value.push_str(next);
            }
            let leading = value.len() - value.trim_start().len();
            value_offset += leading;
            let token = value.trim();
            let id = parse_value(token, value_offset, &mut doc, ctx.limits().max_depth, TEXT)?;
            keys.push((key, id));
            offset += consumed;
        }
        if !keys.is_empty() {
            let mut node = Node::new(Value::Object(keys));
            node.type_hint = Some(TypeHint::new(format!("{PREFIX}config:section")));
            node.flags.insert(NodeFlags::READ_ONLY);
            let id = doc.push(node)?;
            sections.push((section, id));
        }
        let mut root = Node::new(Value::Object(sections));
        root.type_hint = Some(TypeHint::new(format!("{PREFIX}config:root")));
        root.flags.insert(NodeFlags::READ_ONLY);
        let root = doc.push(root)?;
        doc.set_root(root);
        ctx.charge_nodes(len_u64(doc.len()))?;
        let mut params = Params::new();
        params.set("original", input.to_vec());
        Ok(Decoded { doc, params })
    }

    fn encode(&self, doc: &SaveDoc, params: &Params) -> Result<Vec<u8>> {
        let Some(ParamValue::Bytes(original)) = params.get("original") else {
            return Err(Error::new(
                GODOT_WRITE,
                "original ConfigFile text is missing",
            ));
        };
        write_patched(original, doc, TEXT.prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saveprism_core::{Edit, Limits, OwnedValue, Path};

    fn decode(input: &[u8]) -> Decoded {
        let limits = Limits::default();
        let mut ctx = DecodeCtx::new(&limits);
        GodotConfigCodec
            .decode(input, &mut ctx)
            .expect("ConfigFile parses")
    }

    fn encode(decoded: &Decoded) -> Vec<u8> {
        GodotConfigCodec
            .encode(&decoded.doc, &decoded.params)
            .expect("ConfigFile writes")
    }

    fn set(decoded: &mut Decoded, path: &str, value: OwnedValue) {
        saveprism_core::apply_edits(
            &mut decoded.doc,
            &[Edit::Set {
                path: Path::parse(path).expect("path"),
                value,
            }],
            &Limits::default(),
        )
        .expect("edit applies");
    }

    #[test]
    fn sniff_requires_config_like_structure() {
        let codec = GodotConfigCodec;
        let json = SniffCtx {
            file_name: Some("array.save"),
            depth: 0,
        };
        assert_eq!(codec.sniff(b"[1, 2, 3]", &json), Confidence::NONE);
        let config = SniffCtx {
            file_name: Some("settings.cfg"),
            depth: 0,
        };
        assert!(codec.sniff(b"[Player]\nlevel=4\n", &config) > Confidence::NONE);
    }

    #[test]
    fn preserves_config_text_and_patches_only_edited_value() {
        let input = b"; leading comment\n[ Player ]\n  gold   =  42 ; note\nname=\"Mira\\nRune\"\nitems=[1, 2, Vector2(3, 4)]\noptions={\"active\": true}\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/Player/gold", OwnedValue::Int(7));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("  gold   =  7 ; note"));
        assert!(text.contains("; leading comment"));
        assert!(text.contains("Vector2(3, 4)"));
    }

    #[test]
    fn edits_every_scalar_kind_and_keeps_the_rest() {
        let input = b"[Game]\nint = -12\nfloat = 1e10\ntext = \"old\"\nflag = false\n";
        let mut decoded = decode(input);
        set(&mut decoded, "/Game/int", OwnedValue::Int(99));
        set(&mut decoded, "/Game/float", OwnedValue::Float(2.5));
        set(
            &mut decoded,
            "/Game/text",
            OwnedValue::Str("new\nline".into()),
        );
        set(&mut decoded, "/Game/flag", OwnedValue::Bool(true));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert_eq!(
            text,
            "[Game]\nint = 99\nfloat = 2.5\ntext = \"new\\nline\"\nflag = true\n"
        );
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn floats_stay_floats_when_edited() {
        let input = b"[A]\nvalue = 1.25\n";
        let mut decoded = decode(input);
        set(&mut decoded, "/A/value", OwnedValue::Float(2.0));
        let output = encode(&decoded);
        assert_eq!(output, b"[A]\nvalue = 2.0\n");
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn dictionary_keys_stay_nodes_and_constructors_edit_inside() {
        let input = b"[A]\nmap = {\"k\": 1, 2: 3, \"nested\": {}}\npos = Vector2(1, 2)\nname = &\"hero\"\npath = NodePath(\"World/Player\")\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/A/map/0", OwnedValue::Int(9));
        set(&mut decoded, "/A/map/1.key", OwnedValue::Int(7));
        set(&mut decoded, "/A/pos/0", OwnedValue::Float(4.5));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("{\"k\": 9, 7: 3, \"nested\": {}}"));
        assert!(text.contains("Vector2(4.5, 2)"));
        assert!(text.contains("&\"hero\""));
        assert!(text.contains("NodePath(\"World/Player\")"));
        let reread = decode(&output);
        assert!(decoded.doc.tree_eq(&reread.doc));
    }

    #[test]
    fn multi_line_values_and_special_numbers_round_trip() {
        let input = b"; top\n[Odd]\nvalues = [\n  1,\n  2.5, ; inside\n  \"three\"\n]\nmap = {\n  \"a\": 1,\n  \"b\": inf,\n}\nneg = -inf\nnan = nan\nbig = 1e10\n";
        let mut decoded = decode(input);
        assert_eq!(encode(&decoded), input);
        set(&mut decoded, "/Odd/values/1", OwnedValue::Float(9.5));
        let output = encode(&decoded);
        let text = std::str::from_utf8(&output).expect("utf8");
        assert!(text.contains("  9.5, ; inside"));
        assert!(text.contains("; top"));
    }

    #[test]
    fn godot_string_escapes_are_understood() {
        let input = b"[S]\nvalue = \"a\\tb\\u00e9\\U01f600\\\\\\\"\"\n";
        let decoded = decode(input);
        let id = decoded
            .doc
            .resolve(&Path::parse("/S/value").expect("path"))
            .expect("value");
        assert_eq!(
            decoded.doc.get(id).map(|node| &node.value),
            Some(&Value::Str("a\tbé😀\\\"".into()))
        );
        assert_eq!(encode(&decoded), input);
    }

    #[test]
    fn generated_config_fixture_is_byte_exact() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/synthetic/godot/config.cfg");
        let input = std::fs::read(path).expect("fixture exists");
        let decoded = decode(&input);
        assert_eq!(encode(&decoded), input);
    }

    use proptest::prelude::*;

    fn writes_back(bytes: &[u8]) {
        if let Ok(decoded) = GodotConfigCodec.decode(bytes, &mut DecodeCtx::new(&Limits::default()))
        {
            let written = GodotConfigCodec
                .encode(&decoded.doc, &decoded.params)
                .expect("decoded ConfigFile writes");
            let again = GodotConfigCodec
                .decode(&written, &mut DecodeCtx::new(&Limits::default()))
                .expect("written ConfigFile decodes");
            assert!(decoded.doc.tree_eq(&again.doc));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn random_input_never_panics(body in proptest::collection::vec(any::<u8>(), 0..256)) {
            writes_back(&body);
        }

        #[test]
        fn config_like_input_never_panics(
            body in proptest::collection::vec(
                prop_oneof![
                    proptest::sample::select(b"[A]\nk=v;\n\"\\[]{}(),:&".to_vec()),
                    0_u8..=127,
                    any::<u8>(),
                ],
                0..128,
            )
        ) {
            writes_back(&body);
        }
    }
}
