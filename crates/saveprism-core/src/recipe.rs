//! Recipes: the ordered list of steps (layers, then one codec) that turns a file into a document.
//!
//! TOML form (see MIMARI 3.1): `version = 1` first, then one `[[layer]]` table per step with
//! `kind = "..."` and the step parameters as plain fields. The last table is the codec.

use std::fmt;

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result, codes};
use crate::limits::Limits;
use crate::params::{ParamValue, Params, RESERVED_KEY};
use crate::registry::Registry;

/// Largest recipe TOML text that is parsed.
const MAX_RECIPE_TOML_BYTES: usize = 1024 * 1024;

/// One step of a recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// Identifier of the layer or codec (`"compression.gzip"`).
    pub kind: String,
    /// Parameters of the step (the key `kind` is not allowed).
    pub params: Params,
}

impl Step {
    /// Creates a step.
    pub fn new(kind: impl Into<String>, params: Params) -> Self {
        Self {
            kind: kind.into(),
            params,
        }
    }

    /// Creates a step without parameters.
    pub fn bare(kind: impl Into<String>) -> Self {
        Self::new(kind, Params::new())
    }
}

/// A full decode recipe: layers first, the codec last.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Format version of the recipe file; currently 1.
    pub version: u32,
    /// The steps in decode order.
    pub steps: Vec<Step>,
}

impl Recipe {
    /// The only recipe version this build understands.
    pub const CURRENT_VERSION: u32 = 1;

    /// Creates a recipe with the current version.
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            steps,
        }
    }

    /// The step kinds in order.
    pub fn kinds(&self) -> Vec<&str> {
        self.steps.iter().map(|step| step.kind.as_str()).collect()
    }

    /// Checks the recipe against the registry and limits.
    ///
    /// Rules: not empty; supported version; at most `max_layers + 1` steps (`CORE_LIMIT_LAYERS`);
    /// every step but the last is a registered layer and the last is a registered codec
    /// (`CORE_UNKNOWN_STEP` for unknown kinds, `CORE_INVALID_RECIPE` for the wrong kind of step);
    /// no parameter uses the reserved key `kind`.
    pub fn validate(&self, reg: &Registry, limits: &Limits) -> Result<()> {
        if self.version != Self::CURRENT_VERSION {
            return Err(Error::new(
                codes::CORE_INVALID_RECIPE,
                format!(
                    "recipe version {} is not supported (expected {})",
                    self.version,
                    Self::CURRENT_VERSION
                ),
            ));
        }
        let (codec_step, layer_steps) = self
            .steps
            .split_last()
            .ok_or_else(|| Error::new(codes::CORE_INVALID_RECIPE, "the recipe has no steps"))?;
        if layer_steps.len() > limits.max_layers {
            return Err(Error::new(
                codes::CORE_LIMIT_LAYERS,
                format!(
                    "the recipe has {} layers, the limit is {}",
                    layer_steps.len(),
                    limits.max_layers
                ),
            ));
        }
        let total = self.steps.len();
        for (index, step) in layer_steps.iter().enumerate() {
            if !reg.is_layer(&step.kind) {
                return Err(if reg.is_codec(&step.kind) {
                    Error::new(
                        codes::CORE_INVALID_RECIPE,
                        format!(
                            "step {}/{total} ({}) is a codec; only the last step may be one",
                            index + 1,
                            step.kind
                        ),
                    )
                } else {
                    unknown_step(index, total, &step.kind)
                });
            }
            check_step_params(index, total, step)?;
        }
        if !reg.is_codec(&codec_step.kind) {
            let index = layer_steps.len();
            return Err(if reg.is_layer(&codec_step.kind) {
                Error::new(
                    codes::CORE_INVALID_RECIPE,
                    format!(
                        "step {}/{total} ({}) is a layer; the last step must be a codec",
                        index + 1,
                        codec_step.kind
                    ),
                )
            } else {
                unknown_step(index, total, &codec_step.kind)
            });
        }
        check_step_params(layer_steps.len(), total, codec_step)
    }

    /// Writes the recipe as TOML.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string(&RecipeRef(self)).map_err(|e| {
            Error::new(
                codes::CORE_RECIPE_FORMAT,
                format!("the recipe cannot be written as TOML: {e}"),
            )
            .with_source(e)
        })
    }

    /// Reads a recipe from TOML.
    pub fn from_toml(text: &str) -> Result<Recipe> {
        if text.len() > MAX_RECIPE_TOML_BYTES {
            return Err(Error::new(
                codes::CORE_RECIPE_FORMAT,
                format!("the recipe text is larger than {MAX_RECIPE_TOML_BYTES} bytes"),
            ));
        }
        let raw: RecipeDe = toml::from_str(text).map_err(|e| {
            Error::new(
                codes::CORE_RECIPE_FORMAT,
                format!("the recipe is not valid TOML for a recipe: {e}"),
            )
            .with_source(e)
        })?;
        if raw.version != Self::CURRENT_VERSION {
            return Err(Error::new(
                codes::CORE_RECIPE_FORMAT,
                format!(
                    "recipe version {} is not supported (expected {})",
                    raw.version,
                    Self::CURRENT_VERSION
                ),
            ));
        }
        Ok(Recipe {
            version: raw.version,
            steps: raw.layer.into_iter().map(|step| step.0).collect(),
        })
    }
}

fn unknown_step(index: usize, total: usize, kind: &str) -> Error {
    Error::new(
        codes::CORE_UNKNOWN_STEP,
        format!(
            "step {}/{total} uses the unknown kind \"{kind}\"",
            index + 1
        ),
    )
}

fn check_step_params(index: usize, total: usize, step: &Step) -> Result<()> {
    step.params.check_reserved().map_err(|e| {
        e.context(format!(
            "while checking step {}/{total} ({})",
            index + 1,
            step.kind
        ))
    })
}

struct RecipeRef<'a>(&'a Recipe);

impl Serialize for RecipeRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("version", &self.0.version)?;
        map.serialize_entry("layer", &StepsRef(&self.0.steps))?;
        map.end()
    }
}

struct StepsRef<'a>(&'a [Step]);

impl Serialize for StepsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(StepRef))
    }
}

struct StepRef<'a>(&'a Step);

impl Serialize for StepRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        if self.0.params.contains(RESERVED_KEY) {
            return Err(serde::ser::Error::custom(format!(
                "parameter key \"{RESERVED_KEY}\" is reserved for the step kind"
            )));
        }
        let mut map = serializer.serialize_map(Some(self.0.params.len() + 1))?;
        map.serialize_entry("kind", &self.0.kind)?;
        self.0.params.serialize_entries(&mut map)?;
        map.end()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeDe {
    version: u32,
    #[serde(default)]
    layer: Vec<StepDe>,
}

struct StepDe(Step);

struct StepVisitor;

impl<'de> Visitor<'de> for StepVisitor {
    type Value = StepDe;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a recipe step table with a \"kind\" field")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<StepDe, A::Error> {
        let mut kind: Option<String> = None;
        let mut params = Params::new();
        while let Some(key) = map.next_key::<String>()? {
            if key == RESERVED_KEY {
                if kind.is_some() {
                    return Err(de::Error::duplicate_field("kind"));
                }
                kind = Some(map.next_value::<String>()?);
            } else {
                let value = map.next_value::<ParamValue>()?;
                params.set(key, value);
            }
        }
        let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
        Ok(StepDe(Step { kind, params }))
    }
}

impl<'de> Deserialize<'de> for StepDe {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_map(StepVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{Codec, Confidence, DecodeCtx, Decoded, Layer, SniffCtx};
    use crate::doc::SaveDoc;
    use std::sync::Arc;

    fn gzip_like() -> Recipe {
        let mut gzip = Params::new();
        gzip.set("level", 6_u32);
        gzip.set("header", vec![0x1F_u8, 0x8B, 0x08]);
        let mut crypto = Params::new();
        crypto.set("salt", vec![0xDE_u8, 0xAD, 0xBE, 0xEF]);
        crypto.set("iterations", 100_u32);
        crypto.set("key_ref", "vault:example-game");
        Recipe::new(vec![
            Step::new("crypto.es3-aes", crypto),
            Step::new("compression.gzip", gzip),
            Step::bare("format.es3-json"),
        ])
    }

    #[test]
    fn toml_round_trip_is_lossless() {
        let recipe = gzip_like();
        let text = recipe.to_toml().unwrap();
        let back = Recipe::from_toml(&text).unwrap();
        assert_eq!(back, recipe);
    }

    #[test]
    fn toml_has_version_first_and_layer_tables() {
        let text = gzip_like().to_toml().unwrap();
        assert!(text.starts_with("version = 1"), "{text}");
        assert_eq!(text.matches("[[layer]]").count(), 3, "{text}");
        assert!(text.contains("kind = "), "{text}");
        assert!(text.contains("compression.gzip"), "{text}");
        assert!(text.contains("level = 6"), "{text}");
    }

    #[test]
    fn bytes_are_written_as_base64_and_come_back_as_bytes() {
        let text = gzip_like().to_toml().unwrap();
        assert!(text.contains("salt = "), "{text}");
        assert!(text.contains("base64:3q2+7w=="), "{text}");
        let back = Recipe::from_toml(&text).unwrap();
        let crypto = back.steps.first().unwrap();
        assert_eq!(
            crypto.params.bytes("salt").unwrap(),
            [0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    #[test]
    fn text_that_looks_like_bytes_stays_text() {
        let mut params = Params::new();
        params.set("a", "base64:x");
        params.set("b", "str:y");
        params.set("c", "plain");
        let recipe = Recipe::new(vec![Step::new("format.x", params)]);
        let text = recipe.to_toml().unwrap();
        assert!(text.contains("str:base64:x"), "{text}");
        let back = Recipe::from_toml(&text).unwrap();
        let step = back.steps.first().unwrap();
        assert_eq!(step.params.str("a").unwrap(), "base64:x");
        assert_eq!(step.params.str("b").unwrap(), "str:y");
        assert_eq!(step.params.str("c").unwrap(), "plain");
        assert_eq!(back, recipe);
    }

    #[test]
    fn all_param_types_nested_values_and_lists_round_trip() {
        let mut inner = Params::new();
        inner.set("depth", 2_u32);
        inner.set("blob", vec![1_u8, 2, 3]);
        let mut params = Params::new();
        params.set("flag", true);
        params.set("ratio", 0.25_f64);
        params.set("negative", -7_i64);
        params.set(
            "list",
            vec![ParamValue::Int(1), ParamValue::Str("two".into())],
        );
        params.set("table", inner);
        let recipe = Recipe::new(vec![Step::new("format.x", params)]);
        let text = recipe.to_toml().unwrap();
        assert_eq!(Recipe::from_toml(&text).unwrap(), recipe, "{text}");
    }

    #[test]
    fn reading_hand_written_toml() {
        let text = r#"
version = 1

[[layer]]
kind = "compression.gzip"
level = 6

[[layer]]
kind = "format.json"
"#;
        let recipe = Recipe::from_toml(text).unwrap();
        assert_eq!(recipe.kinds(), ["compression.gzip", "format.json"]);
        assert_eq!(recipe.steps[0].params.int("level").unwrap(), 6);
    }

    #[test]
    fn kind_in_params_is_rejected_on_write() {
        let mut params = Params::new();
        params.set("kind", "sneaky");
        let recipe = Recipe::new(vec![Step::new("format.x", params)]);
        let error = recipe.to_toml().unwrap_err();
        assert_eq!(error.code(), codes::CORE_RECIPE_FORMAT);
    }

    #[test]
    fn kind_in_nested_params_is_rejected_on_write() {
        let mut inner = Params::new();
        inner.set("kind", "sneaky");
        let mut params = Params::new();
        params.set("table", inner);
        let recipe = Recipe::new(vec![Step::new("format.x", params)]);
        assert!(recipe.to_toml().is_err());
    }

    #[test]
    fn malformed_toml_is_a_recipe_format_error() {
        for text in [
            "",
            "version = 1\n[[layer]]\nlevel = 1\n",
            "version = 2\n[[layer]]\nkind = \"x\"\n",
            "version = 1\nunknown = 3\n",
            "version = 1\n[[layer]]\nkind = 5\n",
            "version = 1\n[[layer]]\nkind = \"x\"\nsalt = \"base64:***\"\n",
            "this is not toml",
        ] {
            let error = Recipe::from_toml(text).unwrap_err();
            assert_eq!(error.code(), codes::CORE_RECIPE_FORMAT, "{text:?}");
        }
    }

    #[test]
    fn oversized_text_is_rejected() {
        let text = " ".repeat(MAX_RECIPE_TOML_BYTES + 1);
        let error = Recipe::from_toml(&text).unwrap_err();
        assert_eq!(error.code(), codes::CORE_RECIPE_FORMAT);
    }

    // --- validate ---

    struct L(&'static str);

    impl Layer for L {
        fn id(&self) -> &'static str {
            self.0
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<(Vec<u8>, Params)> {
            Ok((input.to_vec(), Params::new()))
        }
        fn encode(&self, inner: &[u8], _params: &Params) -> Result<Vec<u8>> {
            Ok(inner.to_vec())
        }
    }

    struct C(&'static str);

    impl Codec for C {
        fn id(&self) -> &'static str {
            self.0
        }
        fn sniff(&self, _input: &[u8], _ctx: &SniffCtx<'_>) -> Confidence {
            Confidence::NONE
        }
        fn decode(&self, _input: &[u8], _ctx: &mut DecodeCtx<'_>) -> Result<Decoded> {
            Ok(Decoded {
                doc: SaveDoc::new(),
                params: Params::new(),
            })
        }
        fn encode(&self, _doc: &SaveDoc, _params: &Params) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    fn registry() -> Registry {
        let mut reg = Registry::default();
        reg.add_layer(Arc::new(L("layer.a")));
        reg.add_layer(Arc::new(L("layer.b")));
        reg.add_codec(Arc::new(C("codec.x")));
        reg
    }

    fn code_of(recipe: &Recipe) -> crate::error::ErrorCode {
        recipe
            .validate(&registry(), &Limits::default())
            .unwrap_err()
            .code()
    }

    #[test]
    fn valid_recipes_pass() {
        let reg = registry();
        let limits = Limits::default();
        Recipe::new(vec![Step::bare("codec.x")])
            .validate(&reg, &limits)
            .unwrap();
        Recipe::new(vec![
            Step::bare("layer.a"),
            Step::bare("layer.b"),
            Step::bare("codec.x"),
        ])
        .validate(&reg, &limits)
        .unwrap();
    }

    #[test]
    fn empty_recipe_is_invalid() {
        assert_eq!(
            code_of(&Recipe::new(Vec::new())),
            codes::CORE_INVALID_RECIPE
        );
    }

    #[test]
    fn wrong_version_is_invalid() {
        let mut recipe = Recipe::new(vec![Step::bare("codec.x")]);
        recipe.version = 7;
        assert_eq!(code_of(&recipe), codes::CORE_INVALID_RECIPE);
    }

    #[test]
    fn unknown_kinds_are_reported_as_unknown_steps() {
        let first = Recipe::new(vec![Step::bare("nope"), Step::bare("codec.x")]);
        assert_eq!(code_of(&first), codes::CORE_UNKNOWN_STEP);
        let last = Recipe::new(vec![Step::bare("layer.a"), Step::bare("nope")]);
        assert_eq!(code_of(&last), codes::CORE_UNKNOWN_STEP);
    }

    #[test]
    fn step_order_is_enforced() {
        let codec_first = Recipe::new(vec![Step::bare("codec.x"), Step::bare("codec.x")]);
        assert_eq!(code_of(&codec_first), codes::CORE_INVALID_RECIPE);
        let layer_last = Recipe::new(vec![Step::bare("layer.a")]);
        assert_eq!(code_of(&layer_last), codes::CORE_INVALID_RECIPE);
    }

    #[test]
    fn too_many_layers_hit_the_layer_limit() {
        let limits = Limits::default();
        let mut steps: Vec<Step> = (0..limits.max_layers)
            .map(|_| Step::bare("layer.a"))
            .collect();
        steps.push(Step::bare("codec.x"));
        Recipe::new(steps.clone())
            .validate(&registry(), &limits)
            .unwrap();

        steps.insert(0, Step::bare("layer.a"));
        assert_eq!(code_of(&Recipe::new(steps)), codes::CORE_LIMIT_LAYERS);
    }

    #[test]
    fn reserved_param_key_is_invalid_in_validate() {
        let mut params = Params::new();
        params.set("kind", "x");
        let recipe = Recipe::new(vec![Step::new("codec.x", params)]);
        assert_eq!(code_of(&recipe), codes::CORE_INVALID_PARAM);
    }
}
