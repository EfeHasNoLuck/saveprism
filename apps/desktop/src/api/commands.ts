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
  /** An encrypted Easy Save 3 or Terraria file needs its password: ask for one. */
  passwordRequired: ["SP-ES3-005", "SP-TERRARIA-006"] as readonly string[],
  /** The password did not open the file: ask again. */
  wrongPassword: ["SP-ES3-004", "SP-TERRARIA-005"] as readonly string[],
  noDocument: "SP-APP-002",
  readOnlyDocument: "SP-APP-003",
  badValue: "SP-APP-004",
  outOfRange: "SP-APP-005",
  /** "Save as" would replace an existing file: ask, then call `save` with `overwrite: true`. */
  targetExists: "SP-APP-006",
  notEditableNode: "SP-APP-007",
  /** No format matched the file: offer the local file report. */
  unknownFormat: "SP-DET-001",
  /** The pasted save code is empty. */
  emptyCode: "SP-APP-016",
  /** The format is not text, so `copyCode` has no code to give: use Save as. */
  notTextCode: "SP-APP-017",
  /** The picked folder is not a folder, or holds no game data this save can use. */
  dataFolder: "SP-APP-018",
  /** The open file has no game sections that take names from a data folder. */
  noGameSections: "SP-APP-019",
  /** The entry to add is there already. */
  entryExists: "SP-APP-020",
  /** Nothing can be added here. */
  notAddable: "SP-APP-021",
  /** This entry cannot be removed. */
  notRemovable: "SP-APP-022",
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
  /** Text encoding the codec detected (`utf-8`, `shift_jis`, ...), when recorded. */
  textEncoding: string | null;
  guard: GuardLevel;
  /** False when the guard failed (the file is read-only). */
  editable: boolean;
  warning: string | null;
  root: RootInfo;
  /**
   * The engine known from the document's shape ("sugarcube", "renpy", "naninovel"), or null. A pasted code has
   * no file name to tell the engine, so its RPG Maker layer names it ("rpgmaker-mv", "rpgmaker-mz").
   */
  engine:
    | "sugarcube"
    | "renpy"
    | "naninovel"
    | "tyranoscript"
    | "rpgmaker-mv"
    | "rpgmaker-mz"
    | null;
  /** Opened from a pasted save code; `path` is SavePrism's own copy until a "save as". */
  pasted: boolean;
  /** The file is text a player can copy and paste, so `copyCode` can give it back as a code. */
  textCode: boolean;
  /**
   * A notice to show: "renpy-signed" (the game will ask whether to trust the edited save; the
   * player answers Yes), "renpy-key-signed" (the save is signed with the player's own Ren'Py key),
   * "renpy-persistent-key-signed" (the persistent data is signed with the player's own key) or
   * "renpy-persistent-signed" (the file is open read-only) or "terraria-layout" (a Terraria file
   * whose layout is not recognized for this version; open read-only), else null.
   */
  notice:
    | "renpy-signed"
    | "renpy-key-signed"
    | "renpy-persistent-key-signed"
    | "renpy-persistent-signed"
    | "terraria-layout"
    | null;
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
  /** A value can be added to this object (`addValue` with this path as the parent). */
  addable: boolean;
  /** This entry can be removed (`removeValue`). */
  removable: boolean;
}

export interface Page<T> {
  items: T[];
  total: number;
  offset: number;
  /** True when a search stopped at its 10 000 hit limit. */
  truncated: boolean;
}

/** What a pending edit did. */
export type ChangeKind = "set" | "added" | "removed";

export interface Change {
  path: string;
  old: string;
  new: string;
  kind: ChangeKind;
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
  /** The step added or removed entries: read the view again (lists, groups and counts). */
  structural: boolean;
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
 * Opens a file while also looking for the password in the game folder the player picked.
 * Used by the password dialog's "Pick the game folder…" button.
 */
export function openFileWithGameFolder(
  path: string,
  gameFolder: string,
): Promise<OpenInfo> {
  return invoke<OpenInfo>("open_file_with_game_folder", { path, gameFolder });
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
  /** Added since the file was opened (or last saved); `original` is then empty. */
  added: boolean;
}

/** An item, weapon, armor, variable or switch. `name` null: show "Item #5". */
export interface Entry {
  id: number;
  name: string | null;
  field: Field;
  /** True when the entry can be removed from the save. */
  removable: boolean;
}

/** A database entry of the game's data folder. */
export interface NamedId {
  id: number;
  name: string;
}

/** The items, weapons and armors of the game's database, for picking one to add. */
export interface Catalog {
  items: NamedId[];
  weapons: NamedId[];
  armors: NamedId[];
}

/** Which inventory lists take new entries. */
export interface AddLists {
  items: boolean;
  weapons: boolean;
  armors: boolean;
}

/**
 * Stable keys of the stat bonuses: MV/MZ and VX Ace `_paramPlus[0..8]` / `@param_plus`; XP adds
 * strength, dexterity and intelligence, VX and 2000/2003 spirit.
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
  engine: "mv" | "mz" | "xp" | "vx" | "vxace" | "2k" | "2k3";
  gold: Field;
  actors: Actor[];
  items: Entry[];
  weapons: Entry[];
  armors: Entry[];
  variables: Entry[];
  switches: Entry[];
  /** The game data folder the names came from, or null when none was found. */
  namesFolder: string | null;
  /** The game's items, weapons and armors (empty without a data folder). */
  catalog: Catalog;
  /** The inventory lists that take new entries. */
  canAdd: AddLists;
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
  /** Short text of a value without a field or children (read-only); empty otherwise. */
  preview: string;
  /** Why the value is read-only (story view only), else null. */
  note: RowNote | null;
  /** A value can be added to this group (`addValue` with `path` as the parent). */
  addable: boolean;
  /** This row can be removed (`removeValue` with `path`). */
  removable: boolean;
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
  /** Where a new story variable goes (`addValue`), or null when none can be added. */
  addParent: string | null;
}

export type GameView =
  | { kind: "rpgmaker"; view: RpgMakerView }
  | { kind: "sugarcube"; view: SugarCubeView }
  | { kind: "values"; rows: ValueRow[]; addParent: string | null };

/**
 * The game view of the open file: RPG Maker sections for MV, MZ, XP, VX and VX Ace saves, the story
 * view for SugarCube saves, the variables-first list for Naninovel saves, else a flat Values list.
 */
export function gameView(): Promise<GameView> {
  return invoke<GameView>("game_view");
}

/**
 * Reads the names from a game data folder the player picked (the game folder or its data folder)
 * and returns the view with them.
 */
export function setDataFolder(folder: string): Promise<GameView> {
  return invoke<GameView>("set_data_folder", { folder });
}

/** One Gwent card whose name came from the player's own Witcher 3 install. */
export interface W3Card {
  /** The index the save's `W3:gwent-card:<index>` hints carry. */
  index: number;
  /** A readable name from the game's XML `name` (or the `title` key when `name` is missing). */
  name: string;
  /** The XML localization key, kept for a future `.w3strings` reader. */
  title: string;
  /** The XML `faction_index`, e.g. `F_NORTHERN_KINGDOM`. */
  faction: string;
  /** True for a battle-king (leader) card. */
  leader: boolean;
}

export type W3CardsState = "ready" | "pickGameFolder" | "noCards" | "notWitcher3Save";

/** Every Gwent card for the open Witcher 3 save, read from the player's game files. */
export interface W3CardsView {
  state: W3CardsState;
  /** The game folder that was used, when one was found. */
  gameFolder: string | null;
  /** How the folder was found: `save`, `steam`, `gog` or `picked`. */
  source: "save" | "steam" | "gog" | "picked" | null;
  cardCount: number;
  cards: W3Card[];
  /** Bundles or XML files that could not be read (English, for the log). */
  warnings: string[];
}

/**
 * Every Gwent card for the open Witcher 3 save. Names come only from the player's own game files.
 * When no game folder is found the answer has `state: "pickGameFolder"` (never an error): ask the
 * player for the folder and call again with `gameFolder`.
 */
export function w3Cards(gameFolder?: string): Promise<W3CardsView> {
  return invoke<W3CardsView>("w3_cards", { gameFolder: gameFolder ?? null });
}

/**
 * Adds `count` (1 to 99) of the database entry `id` to the party's `list` ("items", "weapons" or
 * "armors") of an RPG Maker save. One undo step.
 */
export function addInventory(list: "items" | "weapons" | "armors", id: number, count: string): Promise<HistoryResult> {
  return invoke<HistoryResult>("add_inventory", { list, id, count });
}

/**
 * Adds the value `key` (of `kind`, typed as `text`) to the object at `parent`: any object of a JSON
 * file, the top of an Easy Save 3 file, or the story variables of a SugarCube save. One undo step.
 */
export function addValue(parent: string, key: string, kind: FieldKind, text: string): Promise<HistoryResult> {
  return invoke<HistoryResult>("add_value", { parent, key, kind, text });
}

/** Removes the value at `path` where the format allows it. One undo step. */
export function removeValue(path: string): Promise<HistoryResult> {
  return invoke<HistoryResult>("remove_value", { path });
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

/** Which side of a comparison a row describes. */
export type CompareKind = "different" | "onlyHere" | "onlyThere";

/** One difference between the open document and another save. */
export interface CompareRow {
  /** JSON Pointer of the value. */
  path: string;
  /** Value in the open document, or null when only the other file has it. */
  here: string | null;
  /** Value in the other file, or null when only the open document has it. */
  there: string | null;
  kind: CompareKind;
}

/** The differences between the open document and another save. */
export interface CompareResult {
  /** The rows to show, at most 5,000. */
  rows: CompareRow[];
  /** Every difference found, including rows left out by the cap. */
  total: number;
  /** True when the cap left differences out. */
  truncated: boolean;
  /** File name of the other save. */
  otherFileName: string;
}

/** One usual save folder of an engine. */
export interface SaveLocationPath {
  /** Path with environment variables expanded; `<game>` / `<company>` stay as placeholders. */
  shown: string;
  /** Folder to open, when it exists on this PC. */
  folder: string | null;
  /** True when `folder` exists. */
  exists: boolean;
}

/** The usual save folders of one engine. */
export interface SaveLocation {
  /** Stable engine id; translate with `locations.engine.<id>`. */
  engine: string;
  paths: SaveLocationPath[];
}

/** One backup of the open file. */
export interface BackupRow {
  /** File name of the backup; pass it to `restoreBackup`. */
  name: string;
  /** When the backup was made, in milliseconds since the Unix epoch. */
  createdMs: number;
  byteSize: number;
}

/** Usual save folders per engine, with the folders that exist on this PC. Nothing is searched. */
export function saveLocations(): Promise<SaveLocation[]> {
  return invoke<SaveLocation[]>("save_location_list");
}

/** Opens a folder from {@link saveLocations} in File Explorer; the host refuses any other path. */
export function openSaveLocation(folder: string): Promise<void> {
  return invoke<void>("open_save_location", { folder });
}

/**
 * Compares the open document (pending edits included) with the file at `path`. The other file is
 * opened read-only with the same detection; encrypted files are refused. Nothing is written.
 */
export function compareWith(path: string): Promise<CompareResult> {
  return invoke<CompareResult>("compare_with", { path });
}

/** The backups of the open file, newest first (empty when it was never saved over). */
export function listBackups(): Promise<BackupRow[]> {
  return invoke<BackupRow[]>("list_backups");
}

/**
 * Puts a backup back in place of the open file and opens the file again. The current file is
 * backed up first, so it shows up in the list as the newest backup. Pending edits are lost.
 * `keepBackups` is the number of backups to keep for the file (1 to 50, 10 when missing).
 */
export function restoreBackup(name: string, keepBackups?: number): Promise<OpenInfo> {
  return invoke<OpenInfo>("restore_backup", { name, keepBackups: keepBackups ?? null });
}

/** A rough guess at what the first bytes of a file are. */
export type EntropyClass = "empty" | "text" | "binary" | "compressed";

/**
 * What the host knows about a file it cannot open, for a GitHub issue. It never holds the path or
 * the file name, only the extension, so the text built from it is safe to paste.
 */
export interface FileReport {
  /** Lower-case extension without the dot; empty when there is none. */
  extension: string;
  byteSize: number;
  /** The first 32 bytes as lower-case hex pairs separated by spaces. */
  headHex: string;
  /** The same bytes as printable ASCII, other bytes as dots. */
  headAscii: string;
  entropy: EntropyClass;
  /** SHA-256 of the whole file, or null for a file above 256 MiB. */
  sha256: string | null;
  /** The SavePrism version that made the report. */
  version: string;
}

/**
 * Reads the report of the regular file at `path` (extension, size, first bytes, SHA-256). The file
 * is only read and nothing is sent anywhere; a folder or link is refused.
 */
export function fileReport(path: string): Promise<FileReport> {
  return invoke<FileReport>("file_report", { path });
}

/** True when the installer includes `THIRD_PARTY_NOTICES.md`. */
export function noticesAvailable(): Promise<boolean> {
  return invoke<boolean>("notices_available");
}

/** Opens the bundled `THIRD_PARTY_NOTICES.md` with the system's default program. */
export function openNotices(): Promise<void> {
  return invoke<void>("open_notices");
}

/** Path of the running SavePrism executable, for the AI app setup snippet. */
export function mcpCommand(): Promise<string> {
  return invoke<string>("mcp_command");
}

/**
 * Saves to `target`. The open file's own path overwrites it (a backup is always made); any other
 * path is "save as" and fails with `ErrorCodes.targetExists` unless `overwrite` is true.
 * `keepBackups` is the number of backups to keep for the file (1 to 50, 10 when missing).
 */
export function save(target: string, overwrite = false, keepBackups?: number): Promise<SaveResult> {
  return invoke<SaveResult>("save", { target, overwrite, keepBackups: keepBackups ?? null });
}
