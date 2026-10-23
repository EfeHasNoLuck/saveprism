// Sizes and grid templates shared by the editor tables.

/** Height of a table row in pixels. */
export const ROW_HEIGHT = 30;

/** #ID · Name · Type · Value · Change · undo. */
export const ENTRY_COLUMNS =
  "grid-cols-[4.25rem_minmax(0,1fr)_4.5rem_13rem_minmax(7.5rem,0.8fr)_1.5rem]";
/** Name · Type · Value · Change · undo (values without an id). */
export const VALUE_COLUMNS = "grid-cols-[minmax(0,1fr)_5.5rem_13rem_minmax(7.5rem,0.8fr)_1.5rem]";

export const ROW = "relative grid h-full items-center gap-x-3 border-b border-border pr-2 pl-3 text-[13px]";

export type RowState = "idle" | "changed" | "error";

/** Background of a row in each state. */
export function rowTone(state: RowState): string {
  if (state === "changed") return "bg-changed-row";
  if (state === "error") return "bg-error-row";
  return "hover:bg-surface-container";
}
