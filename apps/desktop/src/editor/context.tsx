import { createContext, useContext } from "react";
import { isApiError, type ApiError, type Field } from "@/api/commands";

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

/** Turns anything thrown by a command into an `ApiError`. */
export function toApiError(error: unknown, fallbackMessage: string): ApiError {
  if (isApiError(error)) return error;
  return { code: "", message: fallbackMessage, hint: "" };
}
