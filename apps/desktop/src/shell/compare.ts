/** Pure helpers of the "Compare with another save" dialog (no React, no Tauri). */

import type { CompareKind, CompareRow } from "@/api/commands";

/**
 * Rows whose name (from `labels`), path, value here or value there contains `query`
 * (case-insensitive). An empty query keeps every row.
 */
export function filterCompareRows(
  rows: readonly CompareRow[],
  query: string,
  labels?: ReadonlyMap<string, string>,
): CompareRow[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...rows];
  return rows.filter((row) =>
    [labels?.get(row.path) ?? "", row.path, row.here ?? "", row.there ?? ""].some((text) =>
      text.toLowerCase().includes(q),
    ),
  );
}

/** True when the open document has the value, so the row can be shown in the editor. */
export function rowIsHere(row: CompareRow): boolean {
  return row.here !== null;
}

/** The text a value cell shows; an empty cell for the side that does not have the value. */
export function valueCell(value: string | null): string {
  return value ?? "";
}

/**
 * The chip classes of a one-sided badge. The two badges use different surface steps and the
 * changed-colour tokens, so each reads as a chip on a `surface-high` compare row in both themes:
 * muted text on the highest surface for "only here", the changed tag's own text on its container
 * for "only there".
 */
export function kindBadgeClasses(kind: CompareKind): string | null {
  if (kind === "different") return null;
  return kind === "onlyHere"
    ? "bg-surface-highest text-foreground-variant"
    : "bg-changed-tag text-changed-tag-foreground";
}

/**
 * The shown ancestors of a JSON Pointer path, nearest first, ending before the root (`/a/b/c`
 * becomes `["/a/b", "/a"]`).
 */
export function pathAncestors(path: string): string[] {
  const ancestors: string[] = [];
  let current = path;
  for (;;) {
    const cut = current.lastIndexOf("/");
    if (cut <= 0) return ancestors;
    current = current.slice(0, cut);
    ancestors.push(current);
  }
}

/**
 * Which shown path to scroll to for a compare row. A row that names a whole container has no row of
 * its own in the editor, so the nearest shown ancestor is used; when none is shown, the first shown
 * descendant (document order) is, which opens the container's group on the way. `null` when
 * nothing on the way is shown yet.
 */
export function pickRevealTarget(target: string, shown: readonly string[]): string | null {
  if (shown.includes(target)) return target;
  const present = new Set(shown);
  for (const ancestor of pathAncestors(target)) {
    if (present.has(ancestor)) return ancestor;
  }
  const prefix = `${target}/`;
  return shown.find((path) => path.startsWith(prefix)) ?? null;
}
