//! The generic Values list below its top level, and the names a player reads for a path.
//!
//! A group's children become [`ValueRow`]s the way the top level is listed, with what a player does
//! not need folded away: the `{"__type": ..., "value": ...}` wrapper of Easy Save 3, a group with
//! one value and a list with one item show as that value; a map (`[{"key": k, "value": v}]`, the
//! way GVAS maps are written) shows one row per entry, named after its key. A row that can be
//! removed or take new values is not folded, so those actions keep acting on the entry itself.
//!
//! A GVAS save lists its properties (`root.properties`); the rest of the file (`header`,
//! `schemas`, `extra`, `root.save_game_type`) is listed apart as the file details, which the codec
//! keeps read-only.

use std::collections::HashMap;

use saveprism_core::{NodeId, Path, Value};
use serde::Serialize;

use super::game::{At, Bounds, Ctx, UNLIMITED, ValueRow};
use super::{
    ApiError, MAX_PAGE, Page, Session, bad_path, child_at, child_count, preview, value_text,
};

/// Most wrapper levels folded into one row.
const MAX_FOLD: usize = 8;
/// The type entry of an Easy Save 3 value wrapper.
const ES3_TYPE: &str = "__type";
/// The value entry of an Easy Save 3 value wrapper.
const ES3_VALUE: &str = "value";
/// Where a GVAS save keeps its properties.
const GVAS_PROPERTIES: [&str; 2] = ["root", "properties"];
/// The type tables of a GVAS save (file details).
const GVAS_SCHEMAS: &str = "schemas";
/// Separator of the names in a path's label ("Player · Gold").
const LABEL_SEPARATOR: &str = " · ";

/// Most paths [`Session::value_search`] gives back.
const MAX_VALUE_HITS: usize = 5_000;

/// What the Values list's search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValueHits {
    /// Paths of the entries whose own key or value holds the query, in document order (at most
    /// 5,000).
    pub paths: Vec<String>,
    /// Number of entries found; more than `paths` holds when the list was cut.
    pub total: usize,
    /// True when the search stopped at its hit limit, so `total` is a lower bound.
    pub truncated: bool,
}

/// A child row of a group.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ValueChild {
    /// The row as the Values list shows it.
    #[serde(flatten)]
    pub row: ValueRow,
    /// Path of the child's own entry in the group; `row.path` lies below it when wrappers were
    /// folded (a map entry's row edits its value, the entry also holds the key).
    pub entry: String,
}

/// The entries of a map entry list (`[{"key": k, "value": v}, ...]`): each entry's key and value
/// node, or `None` for anything else.
fn map_pairs(ctx: &Ctx<'_>, id: NodeId) -> Option<Vec<(NodeId, NodeId)>> {
    let Value::Array(items) = &ctx.doc.get(id)?.value else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    items
        .iter()
        .map(|item| match &ctx.doc.get(*item)?.value {
            Value::Object(entries) => match entries.as_slice() {
                [(key, key_id), (value, value_id)] if key == "key" && value == ES3_VALUE => {
                    Some((*key_id, *value_id))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

impl Ctx<'_> {
    /// `at` with its wrappers folded away: an Easy Save 3 value wrapper always (`easy_save`); with
    /// `more`, also a group with one value and a list with one item (not a map), unless `addable`
    /// says the group takes new values.
    pub(super) fn fold(
        &self,
        mut at: At,
        easy_save: bool,
        more: bool,
        addable: &dyn Fn(&At) -> bool,
    ) -> At {
        for _ in 0..MAX_FOLD {
            let Some(node) = self.doc.get(at.0) else {
                break;
            };
            let next = match &node.value {
                Value::Object(entries)
                    if easy_save && entries.iter().any(|(k, _)| k == ES3_TYPE) =>
                {
                    self.entry(&at, ES3_VALUE)
                }
                Value::Object(entries) if more && entries.len() == 1 && !addable(&at) => entries
                    .first()
                    .map(|(key, _)| key.clone())
                    .and_then(|key| self.entry(&at, &key)),
                Value::Array(items)
                    if more && items.len() == 1 && map_pairs(self, at.0).is_none() =>
                {
                    items.first().map(|item| {
                        let mut path = at.1.clone();
                        path.push("0");
                        (*item, path)
                    })
                }
                _ => None,
            };
            match next {
                Some(inner) => at = inner,
                None => break,
            }
        }
        at
    }

    /// The text a map entry's key reads as: the value itself, or the one value inside a key that
    /// is a group (a struct key with one field).
    fn key_label(&self, key: NodeId) -> String {
        let mut id = key;
        for _ in 0..MAX_FOLD {
            let next = match self.doc.get(id).map(|node| &node.value) {
                Some(Value::Object(entries)) if entries.len() == 1 => entries.first().map(|e| e.1),
                Some(Value::Array(items)) if items.len() == 1 => items.first().copied(),
                _ => None,
            };
            match next {
                Some(inner) => id = inner,
                None => break,
            }
        }
        self.doc
            .get(id)
            .map(|node| value_text(node).unwrap_or_else(|| preview(node)))
            .unwrap_or_default()
    }

    /// The row of the value or group at `at`, named `name`.
    pub(super) fn row_at(
        &self,
        name: String,
        at: &At,
        bounds: &HashMap<String, Bounds>,
    ) -> ValueRow {
        let node = self.doc.get(at.0);
        let path = at.1.to_string();
        let extra = bounds.get(&path).copied().unwrap_or(UNLIMITED);
        let field = self.field(at, extra);
        ValueRow {
            name,
            preview: match (&field, node) {
                (None, Some(node)) => preview(node),
                _ => String::new(),
            },
            path,
            field,
            children: node.map_or(0, |node| child_count(&node.value)),
            note: None,
            addable: false,
            removable: false,
        }
    }
}

impl Session {
    fn has_step(&self, kind: &str) -> bool {
        self.recipe.steps.iter().any(|step| step.kind == kind)
    }

    /// True for an Easy Save 3 file.
    pub(super) fn is_easy_save(&self) -> bool {
        self.has_step("format.es3")
    }

    /// The properties of a GVAS save (`root.properties`), or `None` for any other file.
    pub(super) fn gvas_properties(&self, ctx: &Ctx<'_>) -> Option<At> {
        if !self.has_step("format.gvas") {
            return None;
        }
        let root: At = (self.doc.root(), Path::root());
        let [save, properties] = GVAS_PROPERTIES;
        let at = ctx.entry(&ctx.entry(&root, save)?, properties)?;
        matches!(self.doc.get(at.0)?.value, Value::Object(_)).then_some(at)
    }

    /// The file details of a GVAS save: the top-level entries beside `root` and the entries of
    /// `root` beside `properties`. Empty for any other file.
    pub(super) fn gvas_details(&self, ctx: &Ctx<'_>) -> Vec<ValueRow> {
        if self.gvas_properties(ctx).is_none() {
            return Vec::new();
        }
        let none = HashMap::new();
        let [save, properties] = GVAS_PROPERTIES;
        let root: At = (self.doc.root(), Path::root());
        let mut rows = Vec::new();
        let mut list = |parent: &At, skip: &str| {
            let Some(Value::Object(entries)) = self.doc.get(parent.0).map(|node| &node.value)
            else {
                return;
            };
            for (key, id) in entries {
                if key == skip {
                    continue;
                }
                let mut path = parent.1.clone();
                path.push(key.as_str());
                rows.push(ctx.row_at(key.clone(), &(*id, path), &none));
            }
        };
        list(&root, save);
        if let Some(save_root) = ctx.entry(&root, save) {
            list(&save_root, properties);
        }
        rows
    }

    /// One page of the children of the group at `path` as the Values list shows them (see the
    /// module notes). `limit` is capped at 500.
    pub fn value_children(
        &self,
        path: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<ValueChild>, ApiError> {
        let limit = limit.min(MAX_PAGE);
        let parent = Path::parse(path)?;
        let id = self.doc.resolve(&parent).ok_or_else(|| bad_path(path))?;
        let total = self.doc.get(id).map_or(0, |node| child_count(&node.value));
        let end = offset.saturating_add(limit).min(total);
        let ctx = self.game_ctx();
        let shape = self.shape_rules(&parent, id);
        let addable = |at: &At| shape.addable(&self.doc, &at.1, at.0);
        let bounds = self.rpg_bounds();
        let easy_save = self.is_easy_save();
        // A map whose entries take no new values shows one row per entry.
        let pairs = map_pairs(&ctx, id).filter(|_| {
            child_at(&self.doc, id, 0).is_none_or(|(segment, child)| {
                let mut entry = parent.clone();
                entry.push(segment);
                !addable(&(child, entry))
            })
        });
        let mut items = Vec::new();
        for index in offset..end {
            let Some((segment, child)) = child_at(&self.doc, id, index) else {
                break;
            };
            let mut entry = parent.clone();
            entry.push(segment.as_str());
            let removable = shape.removable(&self.doc, &segment, child);
            let (name, at) = match pairs.as_ref().and_then(|pairs| pairs.get(index)) {
                Some(&(key, value)) => {
                    let mut value_path = entry.clone();
                    value_path.push(ES3_VALUE);
                    let at = ctx.fold((value, value_path), easy_save, true, &addable);
                    (ctx.key_label(key), at)
                }
                None => {
                    let at = ctx.fold((child, entry.clone()), easy_save, !removable, &addable);
                    (segment, at)
                }
            };
            let mut row = ctx.row_at(name, &at, &bounds);
            row.removable = removable;
            row.addable = addable(&at);
            items.push(ValueChild {
                row,
                entry: entry.to_string(),
            });
        }
        Ok(Page {
            items,
            total,
            offset,
            truncated: false,
        })
    }

    /// Case-insensitive search of the Values list: the entries whose own key (at any depth) or
    /// scalar value holds `query`. Unlike [`Session::search`] the path above an entry does not
    /// count, so a group's name finds the group, not everything in it; the type entry of an Easy
    /// Save 3 wrapper, the containers above a GVAS save's properties and its type tables
    /// (`schemas`, named like the properties) are left out.
    pub fn value_search(&self, query: &str) -> ValueHits {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return ValueHits {
                paths: Vec::new(),
                total: 0,
                truncated: false,
            };
        }
        let ctx = self.game_ctx();
        let base = self.gvas_properties(&ctx).map(|at| at.1);
        let doc = &self.doc;
        let (hits, truncated) = self.collect_by(|id, path| {
            let Some(key) = path.last() else {
                return false;
            };
            let gvas_internal = base.as_ref().is_some_and(|base| {
                base.segments().starts_with(path.segments())
                    || path.segments().first().map(String::as_str) == Some(GVAS_SCHEMAS)
            });
            if key == ES3_TYPE || gvas_internal {
                return false;
            }
            key.to_lowercase().contains(&needle)
                || doc
                    .get(id)
                    .and_then(value_text)
                    .is_some_and(|text| text.to_lowercase().contains(&needle))
        });
        ValueHits {
            total: hits.len(),
            paths: hits
                .into_iter()
                .take(MAX_VALUE_HITS)
                .map(|(_, path)| path.to_string())
                .collect(),
            truncated,
        }
    }

    /// The name a player reads for each path: the names of the rows on the way to it in the Values
    /// list ("STATUS · 0 · currentValue", a map entry by its key), joined with " · ". A GVAS save
    /// starts at its properties. Empty for the root; a path the document does not have keeps its
    /// own keys from where it leaves the document.
    pub fn value_labels(&self, paths: &[String]) -> Vec<String> {
        let ctx = self.game_ctx();
        let shaper = self.shaper(&ctx);
        let addable = |at: &At| shaper.addable(&self.doc, &at.1, at.0);
        let easy_save = self.is_easy_save();
        let base = self.gvas_properties(&ctx);
        let mut maps: HashMap<NodeId, Option<Vec<(NodeId, NodeId)>>> = HashMap::new();
        paths
            .iter()
            .map(|text| {
                let Ok(path) = Path::parse(text) else {
                    return String::new();
                };
                let segments = path.segments();
                let (mut at, mut used) = match &base {
                    Some(base) if segments.starts_with(base.1.segments()) => {
                        (base.clone(), base.1.segments().len())
                    }
                    _ => ((self.doc.root(), Path::root()), 0),
                };
                let mut names: Vec<String> = Vec::new();
                while let Some(segment) = segments.get(used) {
                    let mut entry = at.1.clone();
                    entry.push(segment.as_str());
                    let Some(child) = self.doc.resolve(&entry) else {
                        names.extend(segments[used..].iter().cloned());
                        break;
                    };
                    let pairs = maps.entry(at.0).or_insert_with(|| map_pairs(&ctx, at.0));
                    let pair = segment
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| pairs.as_ref()?.get(index).copied())
                        .filter(|_| !addable(&(child, entry.clone())));
                    let (name, inner) = match pair {
                        Some((key, value)) => {
                            let mut value_path = entry.clone();
                            value_path.push(ES3_VALUE);
                            let inner = ctx.fold((value, value_path), easy_save, true, &addable);
                            (ctx.key_label(key), inner)
                        }
                        None => {
                            let removable =
                                shaper.removable(&self.doc, &at.1, at.0, segment, child, false);
                            let inner =
                                ctx.fold((child, entry.clone()), easy_save, !removable, &addable);
                            (segment.clone(), inner)
                        }
                    };
                    names.push(name);
                    used += 1;
                    let folded = inner.1.segments().get(used..).unwrap_or_default();
                    let rest = &segments[used..];
                    if rest.starts_with(folded) {
                        used += folded.len();
                        at = inner;
                    } else {
                        // The path names a wrapper on the way, or what lies beside its value (a
                        // map entry's key): the rest is named by its own keys.
                        if !folded.starts_with(rest) {
                            names.extend(rest.iter().cloned());
                        }
                        break;
                    }
                }
                names.join(LABEL_SEPARATOR)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path as FsPath;

    use super::super::renpy::RenpyKeySource;
    use super::super::{GameView, Session};

    /// Opens a copy of a synthetic fixture (the copy lives as long as the returned directory).
    fn open(relative: &str) -> (tempfile::TempDir, Session) {
        let dir = tempfile::tempdir().unwrap();
        let source = FsPath::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures")
            .join(relative);
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();
        let target = work.join(source.file_name().unwrap());
        fs::copy(&source, &target).unwrap();
        let session = Session::open_with_keys(
            target.to_str().unwrap(),
            None,
            Some(dir.path().join("backups")),
            RenpyKeySource::At(None),
        )
        .unwrap();
        (dir, session)
    }

    #[test]
    fn gvas_lists_its_properties_and_keeps_the_file_details_read_only() {
        let (_dir, mut session) = open("synthetic/gvas-hints/map-struct-keys.sav");
        let GameView::Values {
            rows,
            add_parent,
            details,
        } = session.game_view().unwrap()
        else {
            panic!("not a Values list");
        };
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["Scores_0"]);
        assert_eq!(rows[0].path, "/root/properties/Scores_0");
        assert_eq!(add_parent, None);
        let details: Vec<&str> = details.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(details, ["header", "schemas", "extra", "save_game_type"]);
        for path in [
            "/header/magic",
            "/header/engine_version",
            "/root/save_game_type",
        ] {
            assert!(!session.field_info(path).unwrap().editable, "{path}");
            assert!(session.edit_value(path, "1").is_err(), "{path}");
        }
        assert!(session.info().editable);
    }

    #[test]
    fn gvas_map_entries_are_rows_named_after_their_read_only_key() {
        let (_dir, mut session) = open("synthetic/gvas-hints/map-struct-keys.sav");
        let page = session
            .value_children("/root/properties/Scores_0", 0, 50)
            .unwrap();
        let rows: Vec<(&str, &str, &str)> = page
            .items
            .iter()
            .map(|child| {
                (
                    child.row.name.as_str(),
                    child.row.path.as_str(),
                    child.entry.as_str(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "alpha",
                    "/root/properties/Scores_0/0/value/Points_0",
                    "/root/properties/Scores_0/0"
                ),
                (
                    "beta",
                    "/root/properties/Scores_0/1/value/Points_0",
                    "/root/properties/Scores_0/1"
                ),
            ]
        );
        let field = page.items[0].row.field.as_ref().unwrap();
        assert_eq!(field.value, "11");
        assert!(field.editable);
        let key = "/root/properties/Scores_0/0/key/Name_0";
        assert!(!session.field_info(key).unwrap().editable);
        assert!(session.edit_value(key, "gamma").is_err());
        session
            .edit_value("/root/properties/Scores_0/0/value/Points_0", "12")
            .unwrap();

        let (_dir, session) = open("synthetic/gvas-hints/map-guid-values.sav");
        let page = session
            .value_children("/root/properties/Ids_0", 0, 50)
            .unwrap();
        let names: Vec<&str> = page.items.iter().map(|c| c.row.name.as_str()).collect();
        assert_eq!(names, ["1", "2"]);
        assert_eq!(page.items[0].row.path, "/root/properties/Ids_0/0/value");
    }

    #[test]
    fn labels_follow_the_rows_on_the_way() {
        let (_dir, session) = open("synthetic/gvas-hints/map-struct-keys.sav");
        let labels = session.value_labels(&[
            "/root/properties/Scores_0/0/value/Points_0".to_string(),
            "/root/properties/Scores_0".to_string(),
            "/header/magic".to_string(),
            "/root/properties/Scores_0/1/key/Name_0".to_string(),
            "/root/properties/Missing_0/3".to_string(),
            "not a path".to_string(),
        ]);
        assert_eq!(
            labels,
            [
                "Scores_0 · alpha",
                "Scores_0",
                "header · magic",
                "Scores_0 · beta · key · Name_0",
                "Missing_0 · 3",
                "",
            ]
        );
    }

    #[test]
    fn search_finds_keys_at_any_depth_and_values_but_not_the_path_above() {
        let (_dir, session) = open("synthetic/gvas-hints/map-struct-keys.sav");
        let hits = session.value_search(" ALPHA ");
        assert_eq!(hits.paths, ["/root/properties/Scores_0/0/key/Name_0"]);
        assert_eq!(hits.total, 1);
        assert!(!hits.truncated);
        let hits = session.value_search("scores");
        assert_eq!(hits.paths, ["/root/properties/Scores_0"]);
        assert!(session.value_search("properties").paths.is_empty());
        assert_eq!(session.value_search("magic").paths, ["/header/magic"]);
        assert!(session.value_search("  ").paths.is_empty());

        let (_dir, session) = open("synthetic/es3/progress.es3");
        assert_eq!(session.value_search("coins").paths, ["/coins"]);
        assert_eq!(session.value_search("250").paths, ["/coins/value"]);
        assert!(session.value_search("System.Int32").paths.is_empty());
    }

    #[test]
    fn easy_save_wrappers_and_one_value_groups_fold_into_one_row() {
        let (_dir, session) = open("synthetic/es3/dict-object-key-pretty.es3");
        let page = session
            .value_children("/LocalPlayerOutfit/value/Items", 0, 50)
            .unwrap();
        let entry = &page.items[0];
        let inner = session.value_children(&entry.row.path, 0, 50).unwrap();
        let guid = inner
            .items
            .iter()
            .find(|child| child.row.name == "guid")
            .unwrap();
        assert_eq!(guid.row.path, format!("{}/guid/value", entry.row.path));
        assert_eq!(guid.entry, format!("{}/guid", entry.row.path));
        assert_eq!(
            guid.row.field.as_ref().unwrap().value,
            "00000000-0000-0000-0000-000000000001"
        );
        let labels = session.value_labels(std::slice::from_ref(&guid.row.path));
        assert_eq!(labels, ["LocalPlayerOutfit · Items · 0 · guid"]);
    }
}
