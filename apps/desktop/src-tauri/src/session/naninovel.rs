//! Naninovel save slots as a Values list.
//!
//! Naninovel (a Unity visual novel engine) writes its game state as JSON with an
//! `objectJsonMap`: service type names in `keys`, their state as JSON strings in `values` (see
//! `saveprism_codecs::naninovel`). The codec inlines those strings, so this view can point at
//! real paths inside them.
//!
//! The view lists the game's custom variables first (`naninovel.CustomVariableManager+GameState`,
//! `LocalVariables` and `GlobalVariables`), by name, then the rest of the save as ordinary rows.
//! `value.type` selects the editable field: 0 text, 1 number, 2 boolean. Nothing is executed.

use saveprism_codecs::naninovel::is_custom_variables;
use saveprism_core::Path;

use super::game::{At, Ctx, MAX_VALUE_ROWS, UNLIMITED, ValueRow};
use super::structure::Shaper;
use super::{Session, child_count};

/// The codec id of a Naninovel slot.
const NANINOVEL_CODEC: &str = "format.naninovel";

/// Variable lists of the custom-variable state and the suffix of their display names.
const VARIABLE_LISTS: [(&str, &str); 2] =
    [("LocalVariables", ""), ("GlobalVariables", " (global)")];

impl Ctx<'_> {
    /// One row of the Naninovel view: a field for a scalar, a group for a container.
    fn naninovel_row(&self, name: String, at: &At) -> Option<ValueRow> {
        let node = self.doc.get(at.0)?;
        Some(ValueRow {
            name,
            path: at.1.to_string(),
            field: self.field(at, UNLIMITED),
            children: child_count(&node.value),
            preview: String::new(),
            note: None,
            addable: false,
            removable: false,
        })
    }

    /// The custom variables, then the rest of the top-level save fields.
    pub(super) fn naninovel_rows(&self, shaper: &Shaper) -> Vec<ValueRow> {
        let root: At = (self.doc.root(), Path::root());
        let mut variables = Vec::new();
        if let Some(map) = self.entry(&root, "objectJsonMap")
            && let Some(keys) = self.entry(&map, "keys")
            && let Some(values) = self.entry(&map, "values")
        {
            let keys = self.items(&keys);
            let values = self.items(&values);
            for (index, state) in values {
                let Some(key) = keys
                    .iter()
                    .find(|(at_index, _)| *at_index == index)
                    .and_then(|(_, at)| self.str_at(at))
                else {
                    continue;
                };
                if !is_custom_variables(key) {
                    continue;
                }
                for (list, suffix) in VARIABLE_LISTS {
                    let Some(list_at) = self.entry(&state, list) else {
                        continue;
                    };
                    for (_, variable) in self.items(&list_at) {
                        let Some(name_at) = self.entry(&variable, "name") else {
                            continue;
                        };
                        let Some(name) = self.str_at(&name_at) else {
                            continue;
                        };
                        let Some(value_at) = self.entry(&variable, "value") else {
                            continue;
                        };
                        let Some(kind_at) = self.entry(&value_at, "type") else {
                            continue;
                        };
                        let field = match self.int_at(&kind_at) {
                            Some(0) => "stringValue",
                            Some(1) => "numericValue",
                            Some(2) => "booleanValue",
                            _ => continue,
                        };
                        let Some(field_at) = self.entry(&value_at, field) else {
                            continue;
                        };
                        // Naninovel names global variables `g_…`; only others get the suffix.
                        let suffix = if name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("g_"))
                        {
                            ""
                        } else {
                            suffix
                        };
                        if let Some(row) = self.naninovel_row(format!("{name}{suffix}"), &field_at)
                        {
                            variables.push(row);
                        }
                    }
                }
            }
        }
        variables.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        });

        let mut rows = variables;
        let room = MAX_VALUE_ROWS.saturating_sub(rows.len());
        rows.extend(self.value_rows(false, shaper).into_iter().take(room));
        rows
    }
}

impl Session {
    /// True when the open document was decoded by the Naninovel codec (by shape, not extension).
    pub(super) fn is_naninovel(&self) -> bool {
        self.recipe
            .steps
            .iter()
            .any(|step| step.kind == NANINOVEL_CODEC)
    }

    /// The Naninovel Values list; `None` for other files.
    pub(super) fn naninovel_view(&self, ctx: &Ctx<'_>) -> Option<Vec<ValueRow>> {
        self.is_naninovel()
            .then(|| ctx.naninovel_rows(&self.shaper(ctx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_lists_are_named() {
        assert_eq!(VARIABLE_LISTS[0], ("LocalVariables", ""));
        assert_eq!(VARIABLE_LISTS[1], ("GlobalVariables", " (global)"));
    }
}
