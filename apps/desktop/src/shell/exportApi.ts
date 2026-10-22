import { invoke } from "@tauri-apps/api/core";

/**
 * Pretty JSON text of the open document (read-only view; the host refuses above 50 MB).
 * Kept here because `src/api/commands.ts` belongs to another task; move it there later.
 */
export function exportJson(): Promise<string> {
  return invoke<string>("export_json");
}
