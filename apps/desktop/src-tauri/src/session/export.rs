//! Pretty JSON text of the open document, for the read-only "Open JSON" view.
//!
//! The text is a view, not a file format: bytes become `"<N bytes>"`, references become `"-> #N"`
//! (so cyclic graphs cannot loop), maps become lists of `{"key", "value"}`, and repeated object keys
//! get a ` (2)`, ` (3)` suffix. Nothing in the document is executed or evaluated.

use saveprism_core::{Error, NodeId, SaveDoc, Value, codes};
use serde_json::{Map, Number, Value as Json};

use super::{ApiError, Session};

/// Largest JSON text the view returns, in bytes.
pub const MAX_EXPORT_BYTES: usize = 50 * 1024 * 1024;
/// Deepest nesting the view follows.
const MAX_DEPTH: usize = 128;

fn too_big(detail: &str) -> ApiError {
    ApiError::new(
        codes::CORE_LIMIT_DECODED.as_str(),
        format!("the JSON view is too large ({detail}); the limit is 50 MB"),
        "Use the tree and search to find the value instead.",
    )
}

fn scalar_int(v: i128) -> Json {
    if let Ok(small) = i64::try_from(v) {
        Json::Number(Number::from(small))
    } else if let Ok(big) = u64::try_from(v) {
        Json::Number(Number::from(big))
    } else {
        Json::String(v.to_string())
    }
}

fn unique_key(map: &Map<String, Json>, key: &str) -> String {
    if !map.contains_key(key) {
        return key.to_string();
    }
    let mut n: usize = 2;
    loop {
        let candidate = format!("{key} ({n})");
        if !map.contains_key(&candidate) {
            return candidate;
        }
        n = n.saturating_add(1);
    }
}

fn convert(doc: &SaveDoc, id: NodeId, depth: usize, budget: &mut usize) -> Result<Json, ApiError> {
    let next = depth.saturating_add(1);
    if depth > MAX_DEPTH {
        return Ok(Json::String("<too deep>".to_string()));
    }
    if *budget == 0 {
        return Err(too_big("too many values"));
    }
    *budget = budget.saturating_sub(1);
    let Some(node) = doc.get(id) else {
        return Ok(Json::Null);
    };
    Ok(match &node.value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int { v, .. } => scalar_int(*v),
        Value::Float { v, .. } => match Number::from_f64(*v) {
            Some(number) => Json::Number(number),
            None => Json::String(format!("{v:?}")),
        },
        Value::Str(text) => Json::String(text.clone()),
        Value::Bytes(bytes) => Json::String(format!("<{} bytes>", bytes.len())),
        Value::Ref(target) => Json::String(format!("-> #{}", target.index())),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len().min(1024));
            for child in items {
                out.push(convert(doc, *child, next, budget)?);
            }
            Json::Array(out)
        }
        Value::Object(entries) => {
            let mut map = Map::new();
            for (key, child) in entries {
                let value = convert(doc, *child, next, budget)?;
                let key = unique_key(&map, key);
                map.insert(key, value);
            }
            Json::Object(map)
        }
        Value::Map(entries) => {
            let mut out = Vec::with_capacity(entries.len().min(1024));
            for (key, child) in entries {
                let mut pair = Map::new();
                pair.insert("key".to_string(), convert(doc, *key, next, budget)?);
                pair.insert("value".to_string(), convert(doc, *child, next, budget)?);
                out.push(Json::Object(pair));
            }
            Json::Array(out)
        }
    })
}

/// Pretty JSON text of `doc`; fails when the text is longer than `max_bytes`.
fn export_doc(doc: &SaveDoc, max_bytes: usize) -> Result<String, ApiError> {
    let mut budget = doc.len();
    let tree = convert(doc, doc.root(), 0, &mut budget)?;
    let text = serde_json::to_string_pretty(&tree).map_err(|e| {
        ApiError::from(Error::new(
            codes::CORE_LIMIT_DECODED,
            format!("the JSON view could not be built: {e}"),
        ))
    })?;
    if text.len() > max_bytes {
        return Err(too_big("text is longer than the limit"));
    }
    Ok(text)
}

impl Session {
    /// Pretty JSON text of the document as it is now (pending edits included). Fails above 50 MB.
    pub fn export_json(&self) -> Result<String, ApiError> {
        export_doc(&self.doc, MAX_EXPORT_BYTES)
    }
}

#[cfg(test)]
mod tests {
    use saveprism_core::{IntWidth, Node};

    use super::*;

    /// `{"name": "Ann", "level": 7, "tags": [true, null], "name": "dup"}`
    fn sample() -> SaveDoc {
        let mut doc = SaveDoc::new();
        let name = doc.push(Node::new(Value::Str("Ann".to_string()))).unwrap();
        let level = doc
            .push(Node::new(Value::Int {
                v: 7,
                width: IntWidth::I32,
            }))
            .unwrap();
        let yes = doc.push(Node::new(Value::Bool(true))).unwrap();
        let none = doc.push(Node::new(Value::Null)).unwrap();
        let tags = doc.push(Node::new(Value::Array(vec![yes, none]))).unwrap();
        let dup = doc.push(Node::new(Value::Str("dup".to_string()))).unwrap();
        let root = doc
            .push(Node::new(Value::Object(vec![
                ("name".to_string(), name),
                ("level".to_string(), level),
                ("tags".to_string(), tags),
                ("name".to_string(), dup),
            ])))
            .unwrap();
        doc.set_root(root);
        doc
    }

    #[test]
    fn small_document_gives_valid_pretty_json() {
        let text = export_doc(&sample(), MAX_EXPORT_BYTES).unwrap();
        assert!(text.contains('\n'), "pretty output has line breaks");
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["name"], "Ann");
        assert_eq!(parsed["level"], 7);
        assert_eq!(parsed["tags"], serde_json::json!([true, null]));
        assert_eq!(parsed["name (2)"], "dup");
    }

    #[test]
    fn text_over_the_limit_is_an_error() {
        let error = export_doc(&sample(), 10).unwrap_err();
        assert_eq!(error.code, codes::CORE_LIMIT_DECODED.as_str());
        assert!(!error.hint.is_empty());
    }

    #[test]
    fn deep_nesting_is_cut_instead_of_overflowing() {
        let mut doc = SaveDoc::new();
        let mut inner = doc.push(Node::new(Value::Null)).unwrap();
        for _ in 0..(MAX_DEPTH + 20) {
            inner = doc.push(Node::new(Value::Array(vec![inner]))).unwrap();
        }
        doc.set_root(inner);
        let text = export_doc(&doc, MAX_EXPORT_BYTES).unwrap();
        assert!(text.contains("<too deep>"));
    }
}
