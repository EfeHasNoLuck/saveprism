/** Pure helpers for save codes: text a game gives the player to copy and paste instead of a file. */
import type { OpenInfo } from "@/api/commands";

/** Default file name of "Save as file…" for a pasted code (its own copy's name means nothing). */
export const PASTED_SAVE_NAME = "save-code.txt";

/** Where the "Save as" dialog starts: the open file, or a plain name for a pasted code. */
export function saveAsDefaultPath(file: Pick<OpenInfo, "path" | "pasted">): string {
  return file.pasted ? PASTED_SAVE_NAME : file.path;
}

/**
 * Where the bottom bar offers "Copy code": the main action for a pasted code, an item of the save
 * menu for other files that are text, else nowhere.
 */
export type CopyCodePlace = "main" | "menu" | "none";

export function copyCodePlace(file: Pick<OpenInfo, "pasted" | "textCode">): CopyCodePlace {
  if (file.pasted) return "main";
  return file.textCode ? "menu" : "none";
}

/** The text of a paste worth opening as a code: the plain text when it is not blank, else null. */
export function pastedText(data: Pick<DataTransfer, "getData"> | null): string | null {
  const text = data?.getData("text/plain") ?? "";
  return text.trim() === "" ? null : text;
}
