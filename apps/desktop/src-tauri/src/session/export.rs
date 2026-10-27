//! Pretty JSON text of the open document, for the JSON editor and "Export .json".
//!
//! The text is a view, not a file format: bytes become `"<N bytes>"`, references become `"-> #N"`
//! (so cyclic graphs cannot loop), maps become lists of `{"key", "value"}`, and repeated object keys
//! get a ` (2)`, ` (3)` suffix. Nothing in the document is executed or evaluated. Edited text comes
//! back through `json_apply`, which walks the same view in the same way.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path as FsPath, PathBuf};

use saveprism_core::{Error, NodeId, SaveDoc, Value, codes};
use serde_json::{Map, Number, Value as Json};

use super::{ApiError, Session, app_codes, same_file};

/// Largest JSON text the view returns, in bytes.
pub const MAX_EXPORT_BYTES: usize = 50 * 1024 * 1024;
/// Deepest nesting the view follows.
pub(super) const MAX_DEPTH: usize = 128;
/// What the view shows below [`MAX_DEPTH`].
pub(super) const TOO_DEEP: &str = "<too deep>";

pub(super) fn too_big(detail: &str) -> ApiError {
    ApiError::new(
        codes::CORE_LIMIT_DECODED.as_str(),
        format!("the JSON view is too large ({detail}); the limit is 50 MB"),
        "Use the tree and search to find the value instead.",
    )
}

pub(super) fn scalar_int(v: i128) -> Json {
    if let Ok(small) = i64::try_from(v) {
        Json::Number(Number::from(small))
    } else if let Ok(big) = u64::try_from(v) {
        Json::Number(Number::from(big))
    } else {
        Json::String(v.to_string())
    }
}

/// The text a float gets in the view: a JSON number, or `"NaN"` / `"inf"` when it is not finite.
pub(super) fn scalar_float(v: f64) -> Json {
    match Number::from_f64(v) {
        Some(number) => Json::Number(number),
        None => Json::String(format!("{v:?}")),
    }
}

/// `"<N bytes>"`
pub(super) fn bytes_text(len: usize) -> String {
    format!("<{len} bytes>")
}

/// `"-> #N"`
pub(super) fn ref_text(target: NodeId) -> String {
    format!("-> #{}", target.index())
}

/// The view names of the keys of one object: a repeated key gets the first free ` (2)`, ` (3)`
/// suffix. The next suffix to try is kept per key, so many repeats of one key stay linear.
#[derive(Default)]
pub(super) struct KeyNames {
    used: HashSet<String>,
    next: HashMap<String, usize>,
}

impl KeyNames {
    pub(super) fn name(&mut self, key: &str) -> String {
        if !self.used.contains(key) {
            self.used.insert(key.to_string());
            return key.to_string();
        }
        // Every suffix below the remembered one is taken already (names are never removed).
        let mut n = self.next.get(key).copied().unwrap_or(2);
        loop {
            let candidate = format!("{key} ({n})");
            n = n.saturating_add(1);
            if !self.used.contains(&candidate) {
                self.used.insert(candidate.clone());
                self.next.insert(key.to_string(), n);
                return candidate;
            }
        }
    }
}

fn convert(doc: &SaveDoc, id: NodeId, depth: usize, budget: &mut usize) -> Result<Json, ApiError> {
    let next = depth.saturating_add(1);
    if depth > MAX_DEPTH {
        return Ok(Json::String(TOO_DEEP.to_string()));
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
        Value::Float { v, .. } => scalar_float(*v),
        Value::Str(text) => Json::String(text.clone()),
        Value::Bytes(bytes) => Json::String(bytes_text(bytes.len())),
        Value::Ref(target) => Json::String(ref_text(*target)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len().min(1024));
            for child in items {
                out.push(convert(doc, *child, next, budget)?);
            }
            Json::Array(out)
        }
        Value::Object(entries) => {
            let mut map = Map::new();
            let mut names = KeyNames::default();
            for (key, child) in entries {
                let value = convert(doc, *child, next, budget)?;
                map.insert(names.name(key), value);
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

/// Writes `bytes` to `target` through a temporary file in the same folder and a rename, so a
/// failed write leaves neither a partial target nor the temporary file behind.
pub(super) fn write_atomic(target: &FsPath, bytes: &[u8]) -> Result<(), ApiError> {
    let dir = match target.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => FsPath::new("."),
    };
    let temp_error = |source: std::io::Error| {
        ApiError::from(
            Error::new(
                codes::IO_TEMP_WRITE,
                format!(
                    "cannot write a temporary file in \"{}\": {source}",
                    dir.display()
                ),
            )
            .with_source(source),
        )
    };
    let mut temp = tempfile::Builder::new()
        .prefix(".saveprism-")
        .suffix(".tmp")
        .tempfile_in(dir)
        .map_err(temp_error)?;
    temp.write_all(bytes)
        .and_then(|()| temp.as_file().sync_all())
        .map_err(temp_error)?;
    // A failed rename drops the temporary file, which deletes it.
    temp.persist(target).map_err(|failed| {
        ApiError::from(
            Error::new(
                codes::IO_REPLACE,
                format!("cannot write \"{}\": {}", target.display(), failed.error),
            )
            .with_source(failed.error),
        )
    })?;
    Ok(())
}

impl Session {
    /// Pretty JSON text of the document as it is now (pending edits included). Fails above 50 MB.
    pub fn export_json(&self) -> Result<String, ApiError> {
        export_doc(&self.doc, MAX_EXPORT_BYTES)
    }

    /// Writes [`Session::export_json`] to `target` as UTF-8 text (temporary file, then rename) and
    /// returns the written path. Refuses the open save file itself, names that do not end in
    /// `.json` (so a typed name can never replace a save of another format or a backup) and an
    /// existing folder.
    pub fn export_json_file(&self, target: &str) -> Result<String, ApiError> {
        let target_path = PathBuf::from(target);
        if same_file(&target_path, &self.path) {
            return Err(ApiError::new(
                app_codes::EXPORT_TARGET,
                format!("\"{}\" is the open save file", target_path.display()),
                "Pick another file name. The export never replaces the save.",
            ));
        }
        let is_json = target_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
        if !is_json {
            return Err(ApiError::new(
                app_codes::EXPORT_TARGET,
                format!("\"{}\" is not a .json file", target_path.display()),
                "Pick a file name that ends in .json.",
            ));
        }
        if fs::metadata(&target_path).is_ok_and(|meta| meta.is_dir()) {
            return Err(ApiError::new(
                app_codes::EXPORT_TARGET,
                format!("\"{}\" is a folder", target_path.display()),
                "Pick a file name inside the folder.",
            ));
        }
        let text = self.export_json()?;
        write_atomic(&target_path, text.as_bytes())?;
        Ok(target_path.to_string_lossy().into_owned())
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
    fn repeated_keys_get_the_first_free_suffix() {
        let mut names = KeyNames::default();
        let shown: Vec<String> = ["a", "a", "a (2)", "a", "b"]
            .iter()
            .map(|key| names.name(key))
            .collect();
        assert_eq!(shown, ["a", "a (2)", "a (2) (2)", "a (3)", "b"]);
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
