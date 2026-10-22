import { createContext, useContext, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { isApiError, type ApiError, type Field } from "@/api/commands";
import type { FormatNumber } from "./helpers";

export interface EditorContextValue {
  readOnly: boolean;
  /** Latest known state of fields that were edited (by path). */
  fields: ReadonlyMap<string, Field>;
  setField: (field: Field) => void;
  reportInvalid: (path: string, invalid: boolean) => void;
  /** Call after a successful edit: refreshes the pending-changes count. */
  afterEdit: () => Promise<void>;
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
