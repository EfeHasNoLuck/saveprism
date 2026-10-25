/** Pure helpers of the "Compare with another save" dialog (no React, no Tauri). */

import type { CompareRow } from "@/api/commands";

/**
 * Rows whose path, value here or value there contains `query` (case-insensitive). An empty query
 * keeps every row.
 */
export function filterCompareRows(rows: readonly CompareRow[], query: string): CompareRow[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...rows];
  return rows.filter((row) =>
    [row.path, row.here ?? "", row.there ?? ""].some((text) =>
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
