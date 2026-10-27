//! The inventories of a Satisfactory save for the game view.
//!
//! The codec opens every `InventoryStack` array of every inventory component and names the item
//! class of each stack (see `saveprism_codecs::satisfactory`). This module walks the world once,
//! maps each component to the object that owns it (its instance name before the last `.`), and
//! lists the non-empty stacks with their editable `NumItems` count. Names are built from the
//! class path only (`.../Desc_IronPlate.Desc_IronPlate_C` becomes "Iron Plate"); no name table is
//! copied. Only counts are editable.

use std::collections::HashMap;

use saveprism_core::{NodeId, Path, Value};
use serde::Serialize;

use super::Session;
use super::game::{At, Bounds, Ctx, Field, ValueRow};

/// The largest stack count the game allows.
const MAX_STACK: i128 = 500;
/// Most stacks the view lists (a large world has a few thousand).
const MAX_STACKS: usize = 50_000;
const STACK_COUNT: Bounds = Bounds::new(Some(0), Some(MAX_STACK));
const PLAYER_CLASS: &str = "Char_Player_C";

/// What an inventory belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InventoryKind {
    /// A player character's inventory.
    Player,
    /// A storage container.
    Storage,
    /// Any other building or actor.
    Other,
}

/// One non-empty stack of an inventory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatisfactoryStack {
    /// The item name built from its class path.
    pub name: String,
    /// The stack count (`NumItems`), editable from 0 to 500.
    pub count: Field,
}

/// One non-empty inventory component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatisfactoryInventory {
    /// Stable id (the component's instance name).
    pub id: String,
    /// What it belongs to.
    pub kind: InventoryKind,
    /// A readable label ("Player inventory", "Storage Container #3").
    pub label: String,
    /// Its non-empty stacks, in save order.
    pub stacks: Vec<SatisfactoryStack>,
}

/// A Satisfactory save: its inventories above the flat Values list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatisfactoryView {
    /// Player, storage and other inventories, in that order.
    pub inventories: Vec<SatisfactoryInventory>,
    /// The top-level values, as [`super::game::GameView::Values`] lists them.
    pub rows: Vec<ValueRow>,
    /// Where a new top-level value goes, or `None` when none can be added.
    pub add_parent: Option<String>,
}

/// "StorageContainerMk1" and `Desc_IronPlate_C` as spaced words, without the code suffixes.
fn prettify_class(text: &str) -> String {
    let mut name = text;
    if let Some((_, after)) = name.rsplit_once(':') {
        name = after;
    }
    if let Some((_, after)) = name.rsplit_once('/') {
        name = after;
    }
    if let Some((_, after)) = name.rsplit_once('.') {
        name = after;
    }
    for prefix in ["BP_", "Build_", "Desc_", "Char_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest;
            break;
        }
    }
    if let Some(rest) = name.strip_suffix("_C") {
        name = rest;
    }
    let mut out = String::with_capacity(name.len() + 4);
    let chars: Vec<char> = name.chars().collect();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch == '_' {
            if !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
            continue;
        }
        if ch.is_uppercase() {
            let prev = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let next = chars.get(index.saturating_add(1)).copied();
            let boundary = prev.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit())
                || (prev.is_some_and(|p| p.is_uppercase())
                    && next.is_some_and(|n| n.is_lowercase()));
            if boundary && !out.ends_with(' ') && !out.is_empty() {
                out.push(' ');
            }
        }
        out.push(ch);
    }
    let out = out.trim().to_string();
    if out.is_empty() {
        text.to_string()
    } else {
        out
    }
}

impl Ctx<'_> {
    /// The object entries at `at` with their paths.
    fn object_children(&self, at: &At) -> Vec<(String, At)> {
        match self.doc.get(at.0).map(|node| &node.value) {
            Some(Value::Object(entries)) => entries
                .iter()
                .map(|(key, child)| {
                    let mut path = at.1.clone();
                    path.push(key.as_str());
                    (key.clone(), (*child, path))
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn string_child(&self, at: &At, key: &str) -> Option<String> {
        let node = self.doc.get(at.0)?;
        let Value::Object(entries) = &node.value else {
            return None;
        };
        let (_, child) = entries.iter().find(|(name, _)| name == key)?;
        match &self.doc.get(*child)?.value {
            Value::Str(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// The non-empty stacks of an inventory component.
    fn stacks_of(&self, stacks: NodeId, stacks_path: &Path) -> Vec<SatisfactoryStack> {
        let mut out = Vec::new();
        let Some(node) = self.doc.get(stacks) else {
            return out;
        };
        let Value::Array(items) = &node.value else {
            return out;
        };
        for (index, stack) in items.iter().enumerate() {
            if out.len() >= MAX_STACKS {
                break;
            }
            let mut stack_path = stacks_path.clone();
            stack_path.push(index.to_string());
            let stack_at: At = (*stack, stack_path);
            let Some(Value::Object(entries)) = self.doc.get(stack_at.0).map(|node| &node.value)
            else {
                continue;
            };
            let Some((_, item)) = entries.iter().find(|(name, _)| name == "Item") else {
                continue;
            };
            let item_at: At = (*item, {
                let mut path = stack_at.1.clone();
                path.push("Item");
                path
            });
            let Some(class_ref) = self
                .string_child(&item_at, "ItemClass")
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let Some((_, count)) = entries.iter().find(|(name, _)| name == "NumItems") else {
                continue;
            };
            let mut count_path = stack_at.1.clone();
            count_path.push("NumItems");
            let count_at: At = (*count, count_path);
            if !self.int_at(&count_at).is_some_and(|value| value > 0) {
                continue;
            }
            let Some(mut field) = self.field(&count_at, STACK_COUNT) else {
                continue;
            };
            field.hint = format!(
                "{} \u{b7} {} is the largest stack size",
                field.hint, MAX_STACK
            );
            out.push(SatisfactoryStack {
                name: prettify_class(&class_ref),
                count: field,
            });
        }
        out
    }

    /// Every non-empty inventory of the world, in player, storage, other order.
    pub(super) fn satisfactory_inventories(&self) -> Vec<SatisfactoryInventory> {
        let root: At = (self.doc.root(), Path::root());
        let Some(body) = self.entry(&root, "Body") else {
            return Vec::new();
        };
        let Some(levels) = self.entry(&body, "Levels") else {
            return Vec::new();
        };
        let level_items = self.items(&levels);

        // Every object's class by its instance name, built in one pass so components resolve to
        // their owner without searching the tree.
        let mut owners: HashMap<String, String> = HashMap::new();
        for (_, level) in &level_items {
            let Some(classes) = self.entry(level, "Classes") else {
                continue;
            };
            for (class, objects) in self.object_children(&classes) {
                for (_, object) in self.items(&objects) {
                    if let Some(instance) = self.string_child(&object, "InstanceName") {
                        owners.entry(instance).or_insert_with(|| class.clone());
                    }
                }
            }
        }

        let mut inventories = Vec::new();
        for (_, level) in &level_items {
            let Some(classes) = self.entry(level, "Classes") else {
                continue;
            };
            for (class, objects) in self.object_children(&classes) {
                if !class.starts_with("FGInventoryComponent") {
                    continue;
                }
                for (_, object) in self.items(&objects) {
                    let Some(props) = self.entry(&object, "Properties") else {
                        continue;
                    };
                    let Some(stacks) = self.entry(&props, "mInventoryStacks") else {
                        continue;
                    };
                    let stacks = self.stacks_of(stacks.0, &stacks.1);
                    if stacks.is_empty() {
                        continue;
                    }
                    let instance = self.string_child(&object, "InstanceName");
                    let owner = instance
                        .as_deref()
                        .and_then(|text| text.rsplit_once('.'))
                        .map(|(owner, _)| owner.to_string());
                    let owner_class = owner.as_ref().and_then(|owner| owners.get(owner));
                    let (kind, label) = match owner_class {
                        Some(class) if class == PLAYER_CLASS => {
                            (InventoryKind::Player, "Player inventory".to_string())
                        }
                        Some(class) if class.contains("Storage") => {
                            (InventoryKind::Storage, prettify_class(class))
                        }
                        Some(class) => (InventoryKind::Other, prettify_class(class)),
                        None => (InventoryKind::Other, prettify_class(&class)),
                    };
                    inventories.push(SatisfactoryInventory {
                        id: instance.unwrap_or_else(|| object.1.to_string()),
                        kind,
                        label,
                        stacks,
                    });
                }
            }
        }

        inventories.sort_by_key(|inventory| match inventory.kind {
            InventoryKind::Player => 0,
            InventoryKind::Storage => 1,
            InventoryKind::Other => 2,
        });

        // Player inventories are numbered apart ("Player inventory", "Player 2 inventory");
        // repeated building labels get " #n" (a single one keeps its bare label).
        let mut players = 0_usize;
        for inventory in &mut inventories {
            if inventory.kind == InventoryKind::Player {
                players = players.saturating_add(1);
                if players > 1 {
                    inventory.label = format!("Player {players} inventory");
                }
            }
        }
        let mut totals: HashMap<String, usize> = HashMap::new();
        for inventory in &inventories {
            *totals.entry(inventory.label.clone()).or_insert(0) += 1;
        }
        let mut seen: HashMap<String, usize> = HashMap::new();
        for inventory in &mut inventories {
            if totals.get(&inventory.label).copied().unwrap_or(0) > 1 {
                let number = seen.entry(inventory.label.clone()).or_insert(0);
                *number += 1;
                inventory.label = format!("{} #{number}", inventory.label);
            }
        }
        inventories
    }
}

impl Session {
    /// True when the open document is a Satisfactory save.
    pub(super) fn is_satisfactory(&self) -> bool {
        self.recipe
            .steps
            .last()
            .is_some_and(|step| step.kind == "format.satisfactory")
    }
}
