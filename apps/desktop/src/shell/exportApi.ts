import { invoke } from "@tauri-apps/api/core";

/**
 * Pretty JSON text of the open document (the view the JSON editor edits; the host refuses above
 * 50 MB). Kept here because `src/api/commands.ts` belongs to another task; move it there later.
 */
export function exportJson(): Promise<string> {
  return invoke<string>("export_json");
}

/**
 * Writes the same text to `target` as UTF-8 and returns the written path. The host refuses the open
 * save file itself, folders and names that do not end in `.json`.
 */
export function exportJsonFile(target: string): Promise<string> {
  return invoke<string>("export_json_file", { target });
}

/**
 * Applies edited text of `exportJson`: only values may change (fields, list items, map keys and
 * placeholders stay). The host checks everything first and applies nothing on an error; the
 * changes become ordinary pending edits. Resolves to the number of values that changed.
 */
export function applyJson(text: string): Promise<number> {
  return invoke<number>("apply_json", { text });
}
