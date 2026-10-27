// The names a player reads for value paths (no React, no Tauri) so it can be tested alone. The
// pending changes and Compare show these instead of internal paths (`/party/_gold`).

import type { Actor, Entry, Field, GameView, ValueRow } from "@/api/commands";
import type { Translate } from "./helpers";

/** Joins the parts of a name ("Platina · Level"), like the host's labels of nested values. */
export const LABEL_SEPARATOR = " · ";

/** The RPG Maker lists whose entries are named by their kind and number. */
export type EntryKind = "item" | "weapon" | "armor" | "variable" | "switch";

/** "Switch #1 Limbo", or "Switch #1" for an entry the game data does not name. */
export function entryLabel(
  kind: EntryKind,
  entry: { id: number; name: string | null },
  t: Translate,
): string {
  const fallback = t(`fallback.${kind}`, { id: entry.id });
  return entry.name === null || entry.name.trim() === "" ? fallback : `${fallback} ${entry.name}`;
}

function actorLabels(actor: Actor, t: Translate, labels: Map<string, string>) {
  const name =
    actor.name.trim() === "" ? `${t("sections.character")} #${actor.id}` : actor.name;
  const add = (field: Field | null, part: string) => {
    if (field !== null) labels.set(field.path, `${name}${LABEL_SEPARATOR}${part}`);
  };
  add(actor.level, t("actor.level"));
  add(actor.hp, t("actor.hp"));
  add(actor.mp, t("actor.mp"));
  add(actor.exp, t("actor.exp"));
  for (const bonus of actor.bonuses) {
    add(bonus.field, t(`stat.${bonus.key}`, { defaultValue: bonus.key }));
  }
}

/**
 * The names the open view gives its values, by path: the RPG Maker sections in the UI language
 * ("Gold", "Platina · Level", "Switch #1 Limbo") and the rows of a Values list by their names.
 * Values deeper in a list are not in it; the host names those (`valueLabels`).
 */
export function viewLabels(view: GameView, t: Translate): Map<string, string> {
  const labels = new Map<string, string>();
  const rows = (list: readonly ValueRow[]) => {
    for (const row of list) labels.set(row.path, row.name);
  };
  switch (view.kind) {
    case "rpgmaker": {
      const sections = view.view;
      if (sections.gold) labels.set(sections.gold.path, t("sections.gold"));
      for (const actor of sections.actors) actorLabels(actor, t, labels);
      const lists: [EntryKind, Entry[]][] = [
        ["item", sections.items],
        ["weapon", sections.weapons],
        ["armor", sections.armors],
        ["variable", sections.variables],
        ["switch", sections.switches],
      ];
      for (const [kind, entries] of lists) {
        for (const entry of entries) labels.set(entry.field.path, entryLabel(kind, entry, t));
      }
      break;
    }
    case "sugarcube":
      rows(view.view.variables);
      break;
    case "witcher3":
    case "satisfactory":
      rows(view.view.rows);
      break;
    case "values":
      rows(view.rows);
      rows(view.details);
      break;
  }
  return labels;
}

/**
 * The name for each path: the view's own name, else the host's (`hostLabels`, same order as
 * `paths`), else the path itself when neither has one.
 */
export function pickLabels(
  paths: readonly string[],
  view: ReadonlyMap<string, string>,
  hostLabels: readonly string[],
): Map<string, string> {
  const out = new Map<string, string>();
  paths.forEach((path, index) => {
    const host = hostLabels[index] ?? "";
    out.set(path, view.get(path) ?? (host !== "" ? host : path));
  });
  return out;
}
