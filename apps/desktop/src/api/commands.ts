// Typed wrappers for the Tauri commands of the desktop host.
// The types are written by hand and must match the serde structs in
// apps/desktop/src-tauri/src/session/mod.rs (camelCase field names).

import { invoke } from "@tauri-apps/api/core";

/** Result of the round-trip guard when a file is opened. */
export type GuardLevel = "exact" | "semantic" | "failed";

/** The type of a node as the tree shows it. */
export type NodeKind =
  | "object"
  | "array"
  | "string"
  | "int"
  | "float"
  | "bool"
  | "null"
  | "bytes"
  | "other";

/** An error as the host sends it. `code` selects the translated text; `message` and `hint` are English. */
export interface ApiError {
  code: string;
  message: string;
  hint: string;
}

/** Error codes the UI reacts to. */
export const ErrorCodes = {
  /** The encrypted Easy Save 3 file needs a password: ask for one. */
  passwordRequired: "SP-ES3-005",
  /** The password did not open the file: ask again. */
  wrongPassword: "SP-ES3-004",
  noDocument: "SP-APP-002",
  readOnlyDocument: "SP-APP-003",
  badValue: "SP-APP-004",
  outOfRange: "SP-APP-005",
  /** "Save as" would replace an existing file: ask, then call `save` with `overwrite: true`. */
  targetExists: "SP-APP-006",
  notEditableNode: "SP-APP-007",
  /** The pasted save code is empty. */
  emptyCode: "SP-APP-016",
  /** The format is not text, so `copyCode` has no code to give: use Save as. */
  notTextCode: "SP-APP-017",
} as const;

export interface RootInfo {
  path: string;
  childCount: number;
}

export interface OpenInfo {
  fileName: string;
  path: string;
  /** Format codec id, e.g. `format.json`. */
  codec: string;
  /** All recipe step ids, layers first. */
  recipe: string[];
  byteSize: number;
  guard: GuardLevel;
  /** False when the guard failed (the file is read-only). */
  editable: boolean;
  warning: string | null;
  root: RootInfo;
  /**
   * The engine known from the document's shape ("sugarcube"), or null. A pasted code has no file
   * name to tell the engine, so its RPG Maker layer names it ("rpgmaker-mv", "rpgmaker-mz").
   */
  engine: "sugarcube" | "rpgmaker-mv" | "rpgmaker-mz" | null;
  /** Opened from a pasted save code; `path` is SavePrism's own copy until a "save as". */
  pasted: boolean;
  /** The file is text a player can copy and paste, so `copyCode` can give it back as a code. */
  textCode: boolean;
}

export interface NodeRow {
  /** JSON Pointer of the node. */
  path: string;
  key: string;
  kind: NodeKind;
  /** At most 120 characters. */
  preview: string;
  childCount: number;
  editable: boolean;
}

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  /** True when a search stopped at its 10 000 hit limit. */
  truncated: boolean;
}

export interface Change {
  path: string;
  old: string;
  new: string;
}

/** Whether there is something to undo or redo. */
export interface HistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

/** What `undo` and `redo` return. */
export interface HistoryResult extends HistoryState {
  /** The values the step changed, as they are now (empty for "nothing to do" and when `reload`). */
  fields: Field[];
  /** The step changed too many values to list: read the document again. */
  reload: boolean;
  /** Number of pending changes now. */
  pending: number;
}

export interface SaveResult {
  writtenPath: string;
  backupPath: string | null;
  verified: boolean;
}

/** True when `value` has the shape of an error sent by the host. */
export function isApiError(value: unknown): value is ApiError {
  if (typeof value !== "object" || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.code === "string" &&
    typeof record.message === "string" &&
    typeof record.hint === "string"
  );
}

export function coreVersion(): Promise<string> {
  return invoke<string>("core_version");
}

/** Opens a file. `password` is only needed for encrypted Easy Save 3 files. */
export function openFile(path: string, password?: string): Promise<OpenInfo> {
  return invoke<OpenInfo>("open_file", { path, password: password ?? null });
}

/**
 * Opens a pasted save code (surrounding whitespace is fine). The host keeps the text as its own file
 * in the app data folder; a code that does not open leaves no file behind and its error has a hint
 * for pasted codes. Empty text fails with `ErrorCodes.emptyCode`.
 */
export function openCode(text: string): Promise<OpenInfo> {
  return invoke<OpenInfo>("open_code", { text });
}

/**
 * The open document as a save code, pending edits included (every edited value is checked in it).
 * Nothing is written and the edits stay pending. Fails with `ErrorCodes.notTextCode` for binary
 * and encrypted formats.
 */
export function copyCode(): Promise<string> {
  return invoke<string>("copy_code");
}

/** One page of the children of the node at `path` (`""` is the root). `limit` is capped at 500. */
export function getChildren(
  path: string,
  offset: number,
  limit: number,
): Promise<Page<NodeRow>> {
  return invoke<Page<NodeRow>>("get_children", { path, offset, limit });
}

/** Case-insensitive search over paths and scalar values. An empty query gives an empty page. */
export function search(
  query: string,
  offset: number,
  limit: number,
): Promise<Page<NodeRow>> {
  return invoke<Page<NodeRow>>("search", { query, offset, limit });
}

/** Changes one scalar; `text` is parsed by the node's current type. */
export function editValue(path: string, text: string): Promise<NodeRow> {
  return invoke<NodeRow>("edit_value", { path, text });
}

/** The kind of an editable value. */
export type FieldKind = "int" | "float" | "bool" | "string";

/** One scalar value with its type and limits. */
export interface Field {
  /** Core path string, passed back to `editValue`. */
  path: string;
  /** Current value as text. */
  value: string;
  /** Value when the file was opened (for "was ..."). */
  original: string;
  kind: FieldKind;
  /** The file format's own type name (Easy Save 3: "Int32"), or null when the format has none. */
  typeName: string | null;
  /** Limits; a limit past +-(2^53-1) arrives clamped to it. */
  min: number | null;
  max: number | null;
  /** English description for logs; the UI builds its own text from `kind`, `min` and `max`. */
  hint: string;
  editable: boolean;
}

/** An item, weapon, armor, variable or switch. `name` null: show "Item #5". */
export interface Entry {
  id: number;
  name: string | null;
  field: Field;
}

/**
 * Stable keys of the stat bonuses: MV/MZ and VX Ace `_paramPlus[0..8]` / `@param_plus`; XP adds
 * strength, dexterity and intelligence, VX spirit.
 */
export type StatKey =
  | "maxHp"
  | "maxMp"
  | "attack"
  | "defense"
  | "magicAttack"
  | "magicDefense"
  | "agility"
  | "luck"
  | "strength"
  | "dexterity"
  | "intelligence"
  | "spirit";

export interface Actor {
  id: number;
  name: string;
  level: Field;
  hp: Field;
  mp: Field;
  exp: Field | null;
  /** Stat bonuses in game order; translate `key` with `stat.<key>` (editor namespace). */
  bonuses: { key: StatKey; field: Field }[];
}

export interface RpgMakerView {
  engine: "mv" | "mz" | "xp" | "vx" | "vxace";
  gold: Field;
  actors: Actor[];
  items: Entry[];
  weapons: Entry[];
  armors: Entry[];
  variables: Entry[];
  switches: Entry[];
  /** The game data folder the names came from, or null when none was found. */
  namesFolder: string | null;
}

/** What a read-only row of the story view holds. */
export type NoteKind = "changed" | "map" | "set" | "date" | "code" | "empty";

/** Why a row cannot be edited in the list, with what the UI shows instead of a value. */
export interface RowNote {
  kind: NoteKind;
  /** Entries of a Map or Set, else null. */
  count: number | null;
  /** The text of a Date as the game wrote it (ISO 8601), else null. */
  text: string | null;
}

/**
 * One row of the Values list. `path` addresses the value (or group); use it with `getChildren`
 * to expand a group. `field` is null for groups and read-only rows.
 */
export interface ValueRow {
  name: string;
  path: string;
  field: Field | null;
  children: number;
  /** Why the value is read-only (story view only), else null. */
  note: RowNote | null;
}

/** A SugarCube (Twine) save: its facts and the story variables at the moment the game resumes at. */
export interface SugarCubeView {
  /** The save's title, or null. */
  title: string | null;
  /** The passage the game resumes at, or null when unknown. */
  passage: string | null;
  /** When the game was saved (milliseconds since 1970), or null. */
  date: number | null;
  /** Number of moments in the history. */
  moments: number;
  /** The moment the game resumes at (0-based). */
  index: number;
  variables: ValueRow[];
}

export type GameView =
  | { kind: "rpgmaker"; view: RpgMakerView }
  | { kind: "sugarcube"; view: SugarCubeView }
  | { kind: "values"; rows: ValueRow[] };

/**
 * The game view of the open file: RPG Maker sections for MV, MZ, XP, VX and VX Ace saves, the story
 * view for SugarCube saves, else a flat Values list.
 */
export function gameView(): Promise<GameView> {
  return invoke<GameView>("game_view");
}

/** Type, limits and original value of the scalar at `path`. */
export function fieldInfo(path: string): Promise<Field> {
  return invoke<Field>("field_info", { path });
}

export function pendingChanges(): Promise<Change[]> {
  return invoke<Change[]>("pending_changes");
}

export function discardChanges(): Promise<void> {
  return invoke<void>("discard_changes");
}

/** Takes back the last action (one edit, "Undo all" or one JSON Apply). */
export function undo(): Promise<HistoryResult> {
  return invoke<HistoryResult>("undo");
}

/** Does again the action that was taken back last. */
export function redo(): Promise<HistoryResult> {
  return invoke<HistoryResult>("redo");
}

export function historyState(): Promise<HistoryState> {
  return invoke<HistoryState>("history_state");
}

/**
 * Changes every value in `paths` to `text` (each parsed by its own type and checked against its own
 * limits) as one undo step. On any error nothing changes. The result lists the values that changed.
 */
export function editValues(paths: string[], text: string): Promise<HistoryResult> {
  return invoke<HistoryResult>("edit_values", { paths, text });
}

/** One backup of the open file. */
export interface BackupRow {
  /** File name of the backup; pass it to `restoreBackup`. */
  name: string;
  /** When the backup was made, in milliseconds since the Unix epoch. */
  createdMs: number;
  byteSize: number;
}

/** The backups of the open file, newest first (empty when it was never saved over). */
export function listBackups(): Promise<BackupRow[]> {
  return invoke<BackupRow[]>("list_backups");
}

/**
 * Puts a backup back in place of the open file and opens the file again. The current file is
 * backed up first, so it shows up in the list as the newest backup. Pending edits are lost.
 */
export function restoreBackup(name: string): Promise<OpenInfo> {
  return invoke<OpenInfo>("restore_backup", { name });
}

/** True when the installer includes `THIRD_PARTY_NOTICES.md`. */
export function noticesAvailable(): Promise<boolean> {
  return invoke<boolean>("notices_available");
}

/** Opens the bundled `THIRD_PARTY_NOTICES.md` with the system's default program. */
export function openNotices(): Promise<void> {
  return invoke<void>("open_notices");
}

/**
 * Saves to `target`. The open file's own path overwrites it (a backup is always made); any other
 * path is "save as" and fails with `ErrorCodes.targetExists` unless `overwrite` is true.
 */
export function save(target: string, overwrite = false): Promise<SaveResult> {
  return invoke<SaveResult>("save", { target, overwrite });
}
