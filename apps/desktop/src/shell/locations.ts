/** Pure helpers of the "Where is my save?" dialog (no React, no Tauri). */

/** The translated names of the path placeholders the host sends. */
export interface PlaceholderLabels {
  gameFolder: string;
  game: string;
  company: string;
}

/**
 * Replaces the host's raw `<game folder>`, `<game>` and `<company>` placeholders with the
 * translated names, keeping the angle brackets (`<Oyun>`). The host text is not changed; this is
 * only for display. A missing label leaves its placeholder as it is.
 */
export function translatePathPlaceholders(path: string, labels: PlaceholderLabels): string {
  const pairs: [string, string][] = [
    ["game folder", labels.gameFolder],
    ["game", labels.game],
    ["company", labels.company],
  ];
  let out = path;
  for (const [placeholder, label] of pairs) {
    const shown = label.trim() === "" ? placeholder : label;
    out = out.split(`<${placeholder}>`).join(`<${shown}>`);
  }
  return out;
}

/**
 * True when the host path still holds one of its placeholders (`<game folder>`, `<game>`,
 * `<company>`). Such a path depends on the game, so the host cannot say whether it exists.
 */
export function hasPathPlaceholder(path: string): boolean {
  return ["<game folder>", "<game>", "<company>"].some((placeholder) =>
    path.includes(placeholder),
  );
}

/** The filter chips of the dialog, each with the engines it shows. */
export interface EngineGroup {
  id: string;
  engines: readonly string[];
}

/** Everything that is not its own chip falls under "Other". */
export const ENGINE_GROUPS: readonly EngineGroup[] = [
  { id: "unity", engines: ["unity"] },
  { id: "unreal", engines: ["unreal"] },
  { id: "rpgmaker-mv", engines: ["rpgmaker-mv"] },
  { id: "rpgmaker-classic", engines: ["rpgmaker-classic"] },
  { id: "renpy", engines: ["renpy"] },
  { id: "godot", engines: ["godot"] },
  { id: "gamemaker", engines: ["gamemaker"] },
] as const;

/** The group a chip filters to: its own id, or "other". */
export function engineGroup(engine: string): string {
  const group = ENGINE_GROUPS.find((entry) => entry.engines.includes(engine));
  return group ? group.id : "other";
}

/** What the search looks at, already translated by the caller. */
export interface SearchableLocation {
  engine: string;
  name: string;
  hint: string;
  paths: readonly (string | { shown: string })[];
}

/**
 * The locations the dialog shows for a query and a chip: a query matches the engine name, the
 * hint and the paths (case-insensitive); a chip keeps only its own engines. `group` is a chip id
 * or "all".
 */
export function filterLocations<T extends SearchableLocation>(
  items: readonly T[],
  query: string,
  group: string,
): T[] {
  const needle = query.trim().toLowerCase();
  return items.filter((item) => {
    if (group !== "all" && engineGroup(item.engine) !== group) return false;
    if (needle === "") return true;
    const haystack = [
      item.name,
      item.hint,
      ...item.paths.map((path) => (typeof path === "string" ? path : path.shown)),
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(needle);
  });
}

/** The two-letter tile in front of an engine card. */
export function engineInitials(engine: string): string {
  switch (engine) {
    case "unity":
      return "U";
    case "unreal":
      return "UE";
    case "rpgmaker-mv":
      return "MV";
    case "rpgmaker-classic":
      return "XP";
    case "renpy":
      return "R'";
    case "godot":
      return "GD";
    case "gamemaker":
      return "GM";
    default: {
      const letters = engine
        .split(/[^a-z0-9]+/i)
        .filter(Boolean)
        .map((part) => part[0] ?? "")
        .join("")
        .toUpperCase();
      return letters.slice(0, 2) || "?";
    }
  }
}

/** A piece of text and whether it matches the search. */
export interface TextPart {
  text: string;
  hit: boolean;
}

/** Splits `text` into the parts that match `query` (case-insensitive) and the parts that do not. */
export function highlightText(text: string, query: string): TextPart[] {
  const needle = query.trim();
  if (needle === "") return [{ text, hit: false }];
  const escaped = needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`(${escaped})`, "gi");
  return text
    .split(pattern)
    .filter((part) => part !== "")
    .map((part) => ({ text: part, hit: part.toLowerCase() === needle.toLowerCase() }));
}

/** A token of a displayed path. */
export type PathTokenKind = "text" | "env" | "placeholder";

export interface PathToken {
  kind: PathTokenKind;
  text: string;
}

/**
 * Splits a shown path into plain text, environment variables (muted) and `<placeholders>`
 * (highlighted), so each can be drawn in its own style.
 */
export function tokenizePath(path: string): PathToken[] {
  const pattern = /(%[A-Za-z_][A-Za-z0-9_]*%|HKEY_[A-Z_]+|<[^>]+>)/g;
  const tokens: PathToken[] = [];
  let last = 0;
  for (const match of path.matchAll(pattern)) {
    const index = match.index ?? 0;
    if (index > last) tokens.push({ kind: "text", text: path.slice(last, index) });
    const text = match[0];
    tokens.push({
      kind: text.startsWith("<") ? "placeholder" : "env",
      text,
    });
    last = index + text.length;
  }
  if (last < path.length) tokens.push({ kind: "text", text: path.slice(last) });
  return tokens;
}

/** True when the path is a Windows registry key rather than a folder. */
export function isRegistryPath(path: string): boolean {
  return /^HKEY_[A-Z_]+/i.test(path);
}
