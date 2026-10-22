/** Pure recent-files list logic. Storage is passed in so it can be tested with a broken one. */

import type { ReadableStorage, WritableStorage } from "./settings";

export interface RecentFile {
  path: string;
  fileName: string;
  /** Milliseconds since 1970 when the file was last opened. */
  openedAt: number;
}

export const RECENT_KEY = "saveprism.recent";
/** The list keeps this many files. */
export const RECENT_MAX = 5;

/** Puts `entry` first, removes an older entry with the same path and keeps at most `RECENT_MAX`. */
export function addRecent(list: readonly RecentFile[], entry: RecentFile): RecentFile[] {
  const rest = list.filter((item) => item.path !== entry.path);
  return [entry, ...rest].slice(0, RECENT_MAX);
}

/** Removes the entry with `path`. */
export function removeRecent(list: readonly RecentFile[], path: string): RecentFile[] {
  return list.filter((item) => item.path !== path);
}

function isRecentFile(value: unknown): value is RecentFile {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.path === "string" &&
    record.path !== "" &&
    typeof record.fileName === "string" &&
    typeof record.openedAt === "number" &&
    Number.isFinite(record.openedAt)
  );
}

/** Parses stored text; garbage gives an empty list. Entries are capped and de-duplicated. */
export function parseRecent(raw: string | null): RecentFile[] {
  if (raw === null) return [];
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(data)) return [];
  const seen = new Set<string>();
  const out: RecentFile[] = [];
  for (const item of data) {
    if (!isRecentFile(item) || seen.has(item.path)) continue;
    seen.add(item.path);
    out.push({ path: item.path, fileName: item.fileName, openedAt: item.openedAt });
    if (out.length >= RECENT_MAX) break;
  }
  return out;
}

export function loadRecent(storage: ReadableStorage | null | undefined): RecentFile[] {
  try {
    return parseRecent(storage ? storage.getItem(RECENT_KEY) : null);
  } catch {
    return [];
  }
}

export function saveRecent(
  list: readonly RecentFile[],
  storage: WritableStorage | null | undefined,
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(RECENT_KEY, JSON.stringify(list.slice(0, RECENT_MAX)));
    return true;
  } catch {
    return false;
  }
}
