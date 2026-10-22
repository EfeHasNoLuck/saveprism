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

export function pendingChanges(): Promise<Change[]> {
  return invoke<Change[]>("pending_changes");
}

export function discardChanges(): Promise<void> {
  return invoke<void>("discard_changes");
}

/**
 * Saves to `target`. The open file's own path overwrites it (a backup is always made); any other
 * path is "save as" and fails with `ErrorCodes.targetExists` unless `overwrite` is true.
 */
export function save(target: string, overwrite = false): Promise<SaveResult> {
  return invoke<SaveResult>("save", { target, overwrite });
}
