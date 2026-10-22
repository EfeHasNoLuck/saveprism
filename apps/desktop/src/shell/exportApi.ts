import { invoke } from "@tauri-apps/api/core";

/**
 * Pretty JSON text of the open document (read-only view; the host refuses above 50 MB).
 * Kept here because `src/api/commands.ts` belongs to another task; move it there later.
 */
export function exportJson(): Promise<string> {
  return invoke<string>("export_json");
}

/**
 * Writes the same text to `target` as UTF-8 and returns the written path. The host refuses the open
 * save file itself and folders.
 */
export function exportJsonFile(target: string): Promise<string> {
  return invoke<string>("export_json_file", { target });
}
