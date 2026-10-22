import { Minus, Plus, Undo2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { editValue, fieldInfo, type ApiError, type Field } from "@/api/commands";
import { cn } from "@/lib/utils";
import { toApiError, useEditorContext } from "./context";
import {
  checkRange,
  fieldHint,
  formatValue,
  hintLine,
  stepValue,
  type RangeProblem,
} from "./helpers";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface Props {
  field: Field;
  /** Show the type hint all the time (Gold). */
  alwaysHint?: boolean;
  /** Show the hint as a floating line while focused, so the row keeps its height (lists). */
  floatingHint?: boolean;
  /** Accessible name of the control. */
  label: string;
  className?: string;
}

/** One editor for every scalar: number (with steppers), switch or text. */
export function FieldEditor({
  field: initial,
  alwaysHint = false,
  floatingHint = false,
  label,
  className,
}: Props) {
  const { t, i18n } = useTranslation("editor");
  const language = i18n.resolvedLanguage ?? "en";
  const format = useMemo(() => {
    const numbers = new Intl.NumberFormat(language);
    return (value: number) => numbers.format(value);
  }, [language]);
  const ctx = useEditorContext();
  const field = ctx.fields.get(initial.path) ?? initial;
  const path = field.path;

  const [draft, setDraft] = useState(field.value);
  const [focused, setFocused] = useState(false);
  const [backendError, setBackendError] = useState<ApiError | null>(null);

  // A new committed value (edit or undo) replaces the draft.
  const [syncedValue, setSyncedValue] = useState(field.value);
  if (syncedValue !== field.value) {
    setSyncedValue(field.value);
    setDraft(field.value);
    setBackendError(null);
  }

  const isBool = field.kind === "bool";
  const isNumber = field.kind === "int" || field.kind === "float";
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
    setDraft(next);
    commit(next);
  }

  function undo() {
    void apply(field.original);
  }

  const originalText = formatValue(field.original, format);
  let hintText = fieldHint(field.kind, field.min, field.max, t, format);
  if (backendError !== null) {
    hintText = `${backendError.message} ${backendError.hint}`.trim();
  } else if (problem === "empty" || problem === "notNumber" || problem === "notWhole") {
    hintText = t(`problem.${problem}`);
  }
  const line = hintLine({
    hint: hintText,
    wasText: t("field.was", { value: originalText }),
    changed: !isBool && changed,
    focused: !isBool && focused,
    always: !isBool && alwaysHint,
    invalid,
  });

  // The undo slot is always there, so the input keeps its width when the button appears.
  const undoButton = changed ? (
    <button
      type="button"
      disabled={disabled}
      onClick={undo}
      aria-label={t("field.undo", { value: originalText })}
      title={t("field.undo", { value: originalText })}
      className="inline-flex size-6 shrink-0 items-center justify-center rounded-md text-primary outline-none hover:bg-primary/10 focus-visible:ring-3 focus-visible:ring-ring/50 disabled:text-muted-foreground"
    >
      <Undo2 className="size-3.5" />
    </button>
  ) : (
    <span className="size-6 shrink-0" aria-hidden="true" />
  );

  const stepperClass =
    "inline-flex size-8 shrink-0 items-center justify-center rounded-md border bg-background text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:bg-muted";

  const controls = (
    <div className={cn("flex items-center gap-1", isBool && "justify-end")}>
      {isBool ? (
        <div className="flex shrink-0 justify-end">
          <Switch
            aria-label={label}
            checked={field.value.toLowerCase() === "true"}
            disabled={disabled}
            onCheckedChange={(checked) => commit(checked ? "true" : "false")}
          />
        </div>
      ) : (
        <>
          {field.kind === "int" ? (
            <button
              type="button"
              aria-label={t("field.decrease")}
              disabled={disabled}
              onClick={() => step(-1)}
              className={stepperClass}
            >
              <Minus className="size-3.5" />
            </button>
          ) : null}
          <input
            type="text"
            inputMode={field.kind === "int" ? "numeric" : isNumber ? "decimal" : "text"}
            aria-label={label}
            aria-invalid={invalid || undefined}
            disabled={disabled}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              setBackendError(null);
            }}
            onFocus={() => setFocused(true)}
            onBlur={() => {
              setFocused(false);
              commit(draft);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                commit(draft);
              } else if (e.key === "Escape") {
                setDraft(field.value);
                setBackendError(null);
              }
            }}
            className={cn(
              "h-8 w-full min-w-0 rounded-md border bg-background px-2 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:cursor-not-allowed disabled:bg-muted",
              isNumber && "text-right tabular-nums",
              changed && !invalid && "border-primary/40 bg-primary/10",
              invalid &&
                "border-destructive bg-destructive/5 focus-visible:border-destructive focus-visible:ring-destructive/20",
            )}
          />
          {field.kind === "int" ? (
            <button
              type="button"
              aria-label={t("field.increase")}
              disabled={disabled}
              onClick={() => step(1)}
              className={stepperClass}
            >
              <Plus className="size-3.5" />
            </button>
          ) : null}
        </>
      )}
      {undoButton}
    </div>
  );

  if (floatingHint) {
    // Table rows: the line floats under the field while it is focused or wrong, so the row
    // keeps its height. "was ..." stays in the undo button's tooltip.
    const floating = line !== null && (focused || invalid) ? line : null;
    return (
      <Tooltip open={floating !== null}>
        <TooltipTrigger asChild>
          <div className={cn("min-w-0", className)}>{controls}</div>
        </TooltipTrigger>
        {floating !== null ? (
          <TooltipContent
            side="bottom"
            align="end"
            sideOffset={4}
            className={cn("max-w-72", floating.error && "text-destructive")}
          >
            <span role={floating.error ? "alert" : undefined}>{floating.text}</span>
          </TooltipContent>
        ) : null}
      </Tooltip>
    );
  }

  return (
    <div className={cn("min-w-0", className)}>
      {controls}
      {line !== null ? (
        <p
          className={cn("mt-1 text-xs", line.error ? "text-destructive" : "text-muted-foreground")}
          role={line.error ? "alert" : undefined}
        >
          {line.text}
        </p>
      ) : null}
    </div>
  );
}
