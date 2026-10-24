/**
 * Own names for unnamed variables and switches, kept per game in local storage. Pure logic (no
 * DOM, no React); storage is passed in, so every function can be tested with a fake or a broken
 * storage object, like the settings.
 */

import type { ReadableStorage, WritableStorage } from "./settings";

export const NAMES_PREFIX = "saveprism.names.";

/** Longest own name, in characters. */
export const MAX_NAME_CHARS = 60;

/** Most own names kept for one game, variables and switches together. */
export const MAX_NAMES = 5000;

export type OwnNameList = "variables" | "switches";

const LISTS: readonly OwnNameList[] = ["variables", "switches"];

/** Own names of one game, by entry id. */
export interface OwnNames {
  variables: Record<string, string>;
  switches: Record<string, string>;
}

/** An empty set of names (a fresh object, so callers may fill it in). */
export function emptyNames(): OwnNames {
  return { variables: {}, switches: {} };
}

/** The storage key of one game's own names. */
export function namesKey(game: string): string {
  return `${NAMES_PREFIX}${game}`;
}

/** The folder of a save path, used as the game key when the save has no data folder. */
export function gameFolder(path: string): string {
  const cut = path.search(/[\\/][^\\/]*$/);
  return cut < 0 ? path : path.slice(0, cut);
}

/** Trims a typed name and cuts it to `MAX_NAME_CHARS`; empty text means "no own name". */
export function cleanName(text: string): string {
  return Array.from(text.trim()).slice(0, MAX_NAME_CHARS).join("");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Parses stored text. Anything malformed is left out; broken JSON gives no names. */
export function parseNames(raw: string | null): OwnNames {
  const names = emptyNames();
  if (raw === null) return names;
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return names;
  }
  if (!isRecord(data)) return names;
  let count = 0;
  for (const list of LISTS) {
    const entries = data[list];
    if (!isRecord(entries)) continue;
    for (const [id, value] of Object.entries(entries)) {
      if (count >= MAX_NAMES) return names;
      if (!/^\d+$/.test(id) || typeof value !== "string") continue;
      const name = cleanName(value);
      if (name === "") continue;
      names[list][id] = name;
      count += 1;
    }
  }
  return names;
}

/** Reads one game's own names; a missing or broken storage gives none. */
export function loadNames(
  storage: ReadableStorage | null | undefined,
  game: string | null | undefined,
): OwnNames {
  if (!game) return emptyNames();
  try {
    return parseNames(storage ? storage.getItem(namesKey(game)) : null);
  } catch {
    return emptyNames();
  }
}

/** Writes one game's own names. Returns false when the storage refused. */
export function saveNames(
  names: OwnNames,
  storage: WritableStorage | null | undefined,
  game: string | null | undefined,
): boolean {
  if (!game) return false;
  try {
    if (!storage) return false;
    storage.setItem(namesKey(game), JSON.stringify(names));
    return true;
  } catch {
    return false;
  }
}

/** The own name of an entry, or null. */
export function nameFor(
  names: OwnNames,
  list: OwnNameList,
  id: number,
): string | null {
  return names[list][String(id)] ?? null;
}

/** Number of own names in a game. */
export function countNames(names: OwnNames): number {
  return LISTS.reduce((total, list) => total + Object.keys(names[list]).length, 0);
}

/**
 * Sets the own name of an entry (an empty name removes it). Returns the same object when the
 * change cannot be made: a key that is not a positive id, or a new name past `MAX_NAMES`.
 */
export function setOwnName(
  names: OwnNames,
  list: OwnNameList,
  id: number,
  text: string,
): OwnNames {
  if (!Number.isInteger(id) || id < 1) return names;
  const key = String(id);
  const name = cleanName(text);
  const next: OwnNames = {
    variables: { ...names.variables },
    switches: { ...names.switches },
  };
  if (name === "") {
    delete next[list][key];
    return next;
  }
  if (!(key in next[list]) && countNames(next) >= MAX_NAMES) return names;
  next[list][key] = name;
  return next;
}

/**
 * The name a list shows for an entry: the game's name, else the own name, else the fallback
 * ("Variable #12"). `fallback` builds the translated text for the id.
 */
export function entryName(
  entry: { id: number; name: string | null },
  own: string | null,
  fallback: (id: number) => string,
): string {
  if (entry.name !== null) return entry.name;
  return own ?? fallback(entry.id);
}
