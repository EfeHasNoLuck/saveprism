import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { editValue, fieldInfo, type ApiError, type Field } from "@/api/commands";
import { toApiError, useEditorContext, useFormatNumber } from "./context";
import {
  changeText,
  checkRange,
  fieldHint,
  formatValue,
  hintLine,
  shownLimits,
  stepValue,
  type HintLine,
  type RangeProblem,
} from "./helpers";

/** The editing state of one scalar field, shared by the grid cells and Gold. */
export interface FieldEdit {
  /** Latest known state of the field. */
  field: Field;
  draft: string;
  setDraft: (text: string) => void;
  /** Escape: back to the committed value. */
  reset: () => void;
  focused: boolean;
  setFocused: (focused: boolean) => void;
  commit: (text: string) => void;
  step: (delta: number) => void;
  undo: () => void;
  disabled: boolean;
  changed: boolean;
  invalid: boolean;
  /** Short reason the typed text cannot be saved ("Max 99,999,999"), or null. */
  message: string | null;
  /** Type and limits in words: "Whole number · 0 to 99,999,999". */
  typeHint: string;
  /** "2,500 → 2,601" when changed, else null. */
  change: string | null;
  /** The file's value as the UI shows it (for "Undo (was …)"). */
  originalText: string;
  /** What a hint line shows now: the error, the type while focused, or "old → new". */
  line: HintLine | null;
}

export function useFieldEdit(initial: Field): FieldEdit {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const ctx = useEditorContext();
  const field = ctx.fields.get(initial.path) ?? initial;
  const path = field.path;

  const [draft, setDraftState] = useState(field.value);
  const [focused, setFocused] = useState(false);
  const [backendError, setBackendError] = useState<ApiError | null>(null);

  // A new committed value (edit or undo) replaces the draft.
  const [syncedValue, setSyncedValue] = useState(field.value);
  if (syncedValue !== field.value) {
    setSyncedValue(field.value);
    setDraftState(field.value);
    setBackendError(null);
  }

  const disabled = ctx.readOnly || !field.editable;
  const changed = field.value !== field.original;
  const problem: RangeProblem | null =
    draft === field.value ? null : checkRange(field.kind, draft, field.min, field.max);
  const invalid = problem !== null || backendError !== null;

  const { reportInvalid } = ctx;
  useEffect(() => {
    reportInvalid(path, invalid);
  }, [invalid, path, reportInvalid]);
  useEffect(() => () => reportInvalid(path, false), [path, reportInvalid]);

  async function apply(text: string) {
    try {
      await editValue(path, text);
      const fresh = await fieldInfo(path);
      ctx.setField(fresh);
      setBackendError(null);
      await ctx.afterEdit();
    } catch (e) {
      setBackendError(toApiError(e, t("errors.unknown")));
    }
  }

  function commit(text: string) {
    if (disabled || text === field.value) return;
    if (checkRange(field.kind, text, field.min, field.max) !== null) return;
    void apply(text);
  }

  function step(delta: number) {
    const base = checkRange(field.kind, draft, field.min, field.max) === null ? draft : field.value;
    const next = stepValue(base, delta, field.min, field.max);
    if (next === null) return;
    setDraftState(next);
    commit(next);
  }

  const typeHint = fieldHint(field.kind, field.min, field.max, t, format);
  const limits = shownLimits(field.min, field.max);
  let message: string | null = null;
  if (backendError !== null) {
    message = `${backendError.message} ${backendError.hint}`.trim();
  } else if (problem === "aboveMax" && limits.max !== null) {
    message = t("problem.aboveMax", { max: format(limits.max) });
  } else if (problem === "belowMin" && limits.min !== null) {
    message = t("problem.belowMin", { min: format(limits.min) });
  } else if (problem !== null) {
    message = problem === "aboveMax" || problem === "belowMin" ? typeHint : t(`problem.${problem}`);
  }
  const change = changed ? changeText(field.kind, field.original, field.value, format) : null;
  const isBool = field.kind === "bool";
  const line = hintLine({
    hint: message ?? typeHint,
    focused: !isBool && focused,
    always: false,
    invalid,
    change,
  });

  return {
    field,
    draft,
    setDraft: (text) => {
      setDraftState(text);
      setBackendError(null);
    },
    reset: () => {
      setDraftState(field.value);
      setBackendError(null);
    },
    focused,
    setFocused,
    commit,
    step,
    undo: () => void apply(field.original),
    disabled,
    changed,
    invalid,
    message,
    typeHint,
    change,
    originalText: formatValue(field.original, format),
    line,
  };
}

export type FieldState = "idle" | "changed" | "error";

export function fieldState(edit: FieldEdit): FieldState {
  return edit.invalid ? "error" : edit.changed ? "changed" : "idle";
}

/** Background of a field's row: violet when changed, red when it cannot be saved. */
export function stateTone(state: FieldState): string {
  if (state === "changed") return "bg-changed-row";
  if (state === "error") return "bg-error-row";
  return "hover:bg-surface-container";
}
