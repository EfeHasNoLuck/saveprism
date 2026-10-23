import { createContext, useContext, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { isApiError, type Actor, type ApiError, type Field } from "@/api/commands";
import type { FormatNumber } from "./helpers";

export interface EditorContextValue {
  readOnly: boolean;
  /** Latest known state of fields that were edited (by path). */
  fields: ReadonlyMap<string, Field>;
  setField: (field: Field) => void;
  reportInvalid: (path: string, invalid: boolean) => void;
  /** Call after a successful edit: refreshes the pending-changes count. */
  afterEdit: () => Promise<void>;
  /** Paths of the values that differ from the file (the host's pending changes). */
  pending: ReadonlySet<string>;
}

export const EditorContext = createContext<EditorContextValue | null>(null);

export function useEditorContext(): EditorContextValue {
  const ctx = useContext(EditorContext);
  if (ctx === null) throw new Error("EditorContext is missing");
  return ctx;
}

/** True when the latest known value of `field` differs from the value in the file. */
export function isChanged(fields: ReadonlyMap<string, Field>, field: Field): boolean {
  const latest = fields.get(field.path) ?? field;
  return latest.value !== latest.original;
}

/** Number formatting in the UI language ("1,234" in English, "1.234" in Turkish). */
export function useFormatNumber(): FormatNumber {
  const { i18n } = useTranslation("editor");
  const language = i18n.resolvedLanguage ?? "en";
  return useMemo(() => {
    const numbers = new Intl.NumberFormat(language);
    return (value: number) => numbers.format(value);
  }, [language]);
}

/** Turns anything thrown by a command into an `ApiError`. */
export function toApiError(error: unknown, fallbackMessage: string): ApiError {
  if (isApiError(error)) return error;
  return { code: "", message: fallbackMessage, hint: "" };
}

/** Number of `paths` that have a pending change. */
export function countPending(pending: ReadonlySet<string>, paths: readonly string[]): number {
  if (pending.size === 0) return 0;
  let count = 0;
  for (const path of paths) if (pending.has(path)) count += 1;
  return count;
}

/** Number of pending changes at `path` or below it. */
export function countPendingUnder(pending: ReadonlySet<string>, path: string): number {
  let count = 0;
  for (const p of pending) if (p === path || p.startsWith(`${path}/`)) count += 1;
  return count;
}

/** Every field of a character, for the change counts. */
export function actorPaths(actor: Actor): string[] {
  const paths = [actor.level.path, actor.hp.path, actor.mp.path, ...actor.bonuses.map((b) => b.field.path)];
  if (actor.exp !== null) paths.push(actor.exp.path);
  return paths;
}
