//! Step parameters: a small typed value tree with a stable TOML-friendly encoding.
//!
//! Encoding rules (used by the recipe TOML):
//! - `Bytes` are written as the text `"base64:<standard, padded>"`.
//! - A `Str` that itself starts with `"base64:"` or `"str:"` is written with an extra `"str:"` prefix,
//!   and the prefix is removed once when reading, so text never turns into bytes by accident.
//! - Integers are `i64`; a `u64` above `i64::MAX` is rejected when reading.
//! - The key `kind` is reserved for the recipe step kind and is rejected in a [`Params`].

use std::collections::BTreeMap;
use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{self, SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result, codes};

const BYTES_PREFIX: &str = "base64:";
const STR_PREFIX: &str = "str:";
/// Key that cannot appear in [`Params`] because the recipe uses it for the step kind.
pub const RESERVED_KEY: &str = "kind";

/// A parameter value.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    /// Boolean.
    Bool(bool),
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit float.
    Float(f64),
    /// Text.
    Str(String),
    /// Raw bytes (salt, key material, ...).
    Bytes(Vec<u8>),
    /// List of values.
    List(Vec<ParamValue>),
    /// Nested table.
    Table(Params),
}

impl ParamValue {
    /// True for values that TOML writes as (sub)tables; they are emitted after plain values.
    fn is_table_like(&self) -> bool {
        match self {
            ParamValue::Table(_) => true,
            ParamValue::List(items) => items.iter().any(ParamValue::is_table_like),
            _ => false,
        }
    }
}

impl From<bool> for ParamValue {
    fn from(value: bool) -> Self {
        ParamValue::Bool(value)
    }
}

impl From<i64> for ParamValue {
    fn from(value: i64) -> Self {
        ParamValue::Int(value)
    }
}

impl From<u32> for ParamValue {
    fn from(value: u32) -> Self {
        ParamValue::Int(i64::from(value))
    }
}

impl From<f64> for ParamValue {
    fn from(value: f64) -> Self {
        ParamValue::Float(value)
    }
}

impl From<String> for ParamValue {
    fn from(value: String) -> Self {
        ParamValue::Str(value)
    }
}

impl From<&str> for ParamValue {
    fn from(value: &str) -> Self {
        ParamValue::Str(value.to_string())
    }
}

impl From<Vec<u8>> for ParamValue {
    fn from(value: Vec<u8>) -> Self {
        ParamValue::Bytes(value)
    }
}

impl From<Vec<ParamValue>> for ParamValue {
    fn from(value: Vec<ParamValue>) -> Self {
        ParamValue::List(value)
    }
}

impl From<Params> for ParamValue {
    fn from(value: Params) -> Self {
        ParamValue::Table(value)
    }
}

/// An ordered (by key) parameter table. The order carries no meaning and the output is stable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(BTreeMap<String, ParamValue>);

impl Params {
    /// Creates an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a key. The reserved key `kind` is accepted here but rejected by validation and serialisation.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<ParamValue>) {
        self.0.insert(key.into(), value.into());
    }

    /// The value of a key.
    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }

    /// True when the key exists.
    pub fn contains(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// True when there is no key.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Iterates over the entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParamValue)> {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }

    /// Fails with `CORE_INVALID_PARAM` when the reserved key `kind` is present.
    pub fn check_reserved(&self) -> Result<()> {
        if self.0.contains_key(RESERVED_KEY) {
            return Err(Error::new(
                codes::CORE_INVALID_PARAM,
                format!("parameter key \"{RESERVED_KEY}\" is reserved for the step kind"),
            ));
        }
        Ok(())
    }

    /// Required boolean.
    pub fn bool(&self, key: &str) -> Result<bool> {
        match self.get(key) {
            Some(ParamValue::Bool(value)) => Ok(*value),
            Some(_) => Err(wrong_type(key, "a boolean")),
            None => Err(missing(key)),
        }
    }

    /// Required integer.
    pub fn int(&self, key: &str) -> Result<i64> {
        match self.get(key) {
            Some(ParamValue::Int(value)) => Ok(*value),
            Some(_) => Err(wrong_type(key, "an integer")),
            None => Err(missing(key)),
        }
    }

    /// Required float; an integer value is accepted and converted.
    pub fn float(&self, key: &str) -> Result<f64> {
        match self.get(key) {
            Some(ParamValue::Float(value)) => Ok(*value),
            Some(ParamValue::Int(value)) => Ok(*value as f64),
            Some(_) => Err(wrong_type(key, "a number")),
            None => Err(missing(key)),
        }
    }

    /// Required text.
    pub fn str(&self, key: &str) -> Result<&str> {
        match self.get(key) {
            Some(ParamValue::Str(value)) => Ok(value.as_str()),
            Some(_) => Err(wrong_type(key, "a string")),
            None => Err(missing(key)),
        }
    }

    /// Required bytes.
    pub fn bytes(&self, key: &str) -> Result<&[u8]> {
        match self.get(key) {
            Some(ParamValue::Bytes(value)) => Ok(value.as_slice()),
            Some(_) => Err(wrong_type(key, "a byte string")),
            None => Err(missing(key)),
        }
    }

    /// Optional boolean: `Ok(None)` when absent, an error when present with another type.
    pub fn opt_bool(&self, key: &str) -> Result<Option<bool>> {
        match self.get(key) {
            None => Ok(None),
            Some(_) => self.bool(key).map(Some),
        }
    }

    /// Optional integer.
    pub fn opt_int(&self, key: &str) -> Result<Option<i64>> {
        match self.get(key) {
            None => Ok(None),
            Some(_) => self.int(key).map(Some),
        }
    }

    /// Optional float.
    pub fn opt_float(&self, key: &str) -> Result<Option<f64>> {
        match self.get(key) {
            None => Ok(None),
            Some(_) => self.float(key).map(Some),
        }
    }

    /// Optional text.
    pub fn opt_str(&self, key: &str) -> Result<Option<&str>> {
        match self.get(key) {
            None => Ok(None),
            Some(_) => self.str(key).map(Some),
        }
    }

    /// Optional bytes.
    pub fn opt_bytes(&self, key: &str) -> Result<Option<&[u8]>> {
        match self.get(key) {
            None => Ok(None),
            Some(_) => self.bytes(key).map(Some),
        }
    }

    /// Writes plain values first and table-like values last (TOML requires that order).
    pub(crate) fn serialize_entries<M: SerializeMap>(
        &self,
        map: &mut M,
    ) -> std::result::Result<(), M::Error> {
        for (key, value) in self.0.iter().filter(|(_, value)| !value.is_table_like()) {
            map.serialize_entry(key, value)?;
        }
        for (key, value) in self.0.iter().filter(|(_, value)| value.is_table_like()) {
            map.serialize_entry(key, value)?;
        }
        Ok(())
    }
}

fn missing(key: &str) -> Error {
    Error::new(
        codes::CORE_INVALID_PARAM,
        format!("parameter \"{key}\" is missing"),
    )
}

fn wrong_type(key: &str, expected: &str) -> Error {
    Error::new(
        codes::CORE_INVALID_PARAM,
        format!("parameter \"{key}\" must be {expected}"),
    )
}

impl Serialize for ParamValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            ParamValue::Bool(value) => serializer.serialize_bool(*value),
            ParamValue::Int(value) => serializer.serialize_i64(*value),
            ParamValue::Float(value) => serializer.serialize_f64(*value),
            ParamValue::Str(value) => {
                if value.starts_with(BYTES_PREFIX) || value.starts_with(STR_PREFIX) {
                    serializer.serialize_str(&format!("{STR_PREFIX}{value}"))
                } else {
                    serializer.serialize_str(value)
                }
            }
            ParamValue::Bytes(value) => {
                serializer.serialize_str(&format!("{BYTES_PREFIX}{}", STANDARD.encode(value)))
            }
            ParamValue::List(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            ParamValue::Table(table) => table.serialize(serializer),
        }
    }
}

impl Serialize for Params {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        if self.0.contains_key(RESERVED_KEY) {
            return Err(ser::Error::custom(format!(
                "parameter key \"{RESERVED_KEY}\" is reserved for the step kind"
            )));
        }
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        self.serialize_entries(&mut map)?;
        map.end()
    }
}

struct ParamValueVisitor;

impl<'de> Visitor<'de> for ParamValueVisitor {
    type Value = ParamValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a boolean, integer, float, string, list or table")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<ParamValue, E> {
        Ok(ParamValue::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> std::result::Result<ParamValue, E> {
        Ok(ParamValue::Int(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> std::result::Result<ParamValue, E> {
        i64::try_from(value).map(ParamValue::Int).map_err(|_| {
            E::custom(format!(
                "integer {value} does not fit in a signed 64-bit value"
            ))
        })
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> std::result::Result<ParamValue, E> {
        Ok(ParamValue::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> std::result::Result<ParamValue, E> {
        if let Some(encoded) = value.strip_prefix(BYTES_PREFIX) {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|e| E::custom(format!("invalid base64 value: {e}")))?;
            Ok(ParamValue::Bytes(bytes))
        } else if let Some(text) = value.strip_prefix(STR_PREFIX) {
            Ok(ParamValue::Str(text.to_string()))
        } else {
            Ok(ParamValue::Str(value.to_string()))
        }
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<ParamValue, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<ParamValue>()? {
            items.push(item);
        }
        Ok(ParamValue::List(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> std::result::Result<ParamValue, A::Error> {
        read_params(map).map(ParamValue::Table)
    }
}

impl<'de> Deserialize<'de> for ParamValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(ParamValueVisitor)
    }
}

fn read_params<'de, A: MapAccess<'de>>(mut map: A) -> std::result::Result<Params, A::Error> {
    let mut params = Params::new();
    while let Some(key) = map.next_key::<String>()? {
        if key == RESERVED_KEY {
            return Err(de::Error::custom(format!(
                "parameter key \"{RESERVED_KEY}\" is reserved for the step kind"
            )));
        }
        let value = map.next_value::<ParamValue>()?;
        params.0.insert(key, value);
    }
    Ok(params)
}

struct ParamsVisitor;

impl<'de> Visitor<'de> for ParamsVisitor {
    type Value = Params;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a table of parameters")
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> std::result::Result<Params, A::Error> {
        read_params(map)
    }
}

impl<'de> Deserialize<'de> for Params {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_map(ParamsVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Params {
        let mut params = Params::new();
        params.set("flag", true);
        params.set("level", 6_u32);
        params.set("big", -5_i64);
        params.set("ratio", 0.5_f64);
        params.set("name", "gzip");
        params.set("salt", vec![0xDE_u8, 0xAD, 0xBE, 0xEF]);
        params
    }

    #[test]
    fn typed_reads_succeed() {
        let params = sample();
        assert!(params.bool("flag").unwrap());
        assert_eq!(params.int("level").unwrap(), 6);
        assert_eq!(params.int("big").unwrap(), -5);
        assert_eq!(params.float("ratio").unwrap(), 0.5);
        assert_eq!(params.float("level").unwrap(), 6.0);
        assert_eq!(params.str("name").unwrap(), "gzip");
        assert_eq!(params.bytes("salt").unwrap(), [0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(params.len(), 6);
        assert!(!params.is_empty());
        assert!(params.contains("salt"));
    }

    #[test]
    fn missing_and_wrong_type_name_the_key() {
        let params = sample();
        for error in [
            params.int("absent").unwrap_err(),
            params.int("name").unwrap_err(),
            params.str("level").unwrap_err(),
            params.bytes("name").unwrap_err(),
            params.bool("level").unwrap_err(),
            params.float("name").unwrap_err(),
        ] {
            assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
        }
        assert!(
            params
                .int("absent")
                .unwrap_err()
                .message()
                .contains("absent")
        );
        assert!(params.int("name").unwrap_err().message().contains("name"));
    }

    #[test]
    fn optional_reads() {
        let params = sample();
        assert_eq!(params.opt_int("level").unwrap(), Some(6));
        assert_eq!(params.opt_int("absent").unwrap(), None);
        assert_eq!(params.opt_str("absent").unwrap(), None);
        assert_eq!(params.opt_bool("flag").unwrap(), Some(true));
        assert_eq!(params.opt_float("ratio").unwrap(), Some(0.5));
        assert_eq!(params.opt_bytes("salt").unwrap().map(<[u8]>::len), Some(4));
        let error = params.opt_int("name").unwrap_err();
        assert_eq!(error.code(), codes::CORE_INVALID_PARAM);
    }

    #[test]
    fn from_impls_cover_the_documented_types() {
        let mut params = Params::new();
        params.set("a", true);
        params.set("b", 7_i64);
        params.set("c", 8_u32);
        params.set("d", 1.25_f64);
        params.set("e", String::from("s"));
        params.set("f", "t");
        params.set("g", vec![1_u8, 2]);
        assert_eq!(params.len(), 7);
        assert_eq!(params.get("c"), Some(&ParamValue::Int(8)));
    }

    #[test]
    fn iteration_is_in_key_order() {
        let params = sample();
        let keys: Vec<&str> = params.iter().map(|(key, _)| key).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn check_reserved_rejects_kind() {
        let mut params = Params::new();
        params.set("kind", "x");
        assert_eq!(
            params.check_reserved().unwrap_err().code(),
            codes::CORE_INVALID_PARAM
        );
        assert!(sample().check_reserved().is_ok());
    }
}
