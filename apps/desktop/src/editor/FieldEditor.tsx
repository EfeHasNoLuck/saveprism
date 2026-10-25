import { Minus, Plus, Undo2 } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Field } from "@/api/commands";
import { cn } from "@/lib/utils";
import { RemoveValueButton } from "./RemoveValue";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useFormatNumber } from "./context";
import { changeValue } from "./helpers";
import { BoolChoice } from "./BoolChoice";
import {
  fieldState,
  stateTone,
  useFieldEdit,
  type FieldEdit,
  type FieldState,
} from "./useFieldEdit";

/** The violet (changed) or red (cannot be saved) bar on the left edge of a row. */
export function StateBar({ state }: { state: FieldState }) {
  if (state === "idle") return null;
  return (
    <span
      aria-hidden="true"
      className={cn(
        "absolute inset-y-0 left-0 w-[3px] rounded-l-sm",
        state === "changed" ? "bg-changed" : "bg-destructive",
      )}
    />
  );
}

const STEP =
  "inline-flex h-6.5 w-6 items-center justify-center text-muted-foreground outline-none hover:bg-surface-container hover:text-foreground focus-visible:bg-surface-container focus-visible:text-foreground disabled:pointer-events-none";

/**
 * The value control of a field: a text box or the `true | false` choice. A changed value is
 * violet, a value that cannot be saved is red. Arrow up and down step a number by one; `steppers`
 * adds − and + buttons for whole numbers (Gold).
 */
export function ValueInput({
  edit,
  label,
  steppers = false,
  inputClassName,
  className,
}: {
  edit: FieldEdit;
  /** Accessible name of the control. */
  label: string;
  steppers?: boolean;
  /** Extra classes of the text box (accent colour). */
  inputClassName?: string;
  className?: string;
}) {
  const { t } = useTranslation("editor");
  const { field, disabled, changed, invalid } = edit;
  const isNumber = field.kind === "int" || field.kind === "float";

  if (field.kind === "bool") {
    return (
      <BoolChoice
        label={label}
        value={field.value.toLowerCase() === "true"}
        disabled={disabled}
        changed={changed}
        onChange={(next) => edit.commit(next ? "true" : "false")}
      />
    );
  }

  return (
    <div className={cn("flex min-w-0 items-center gap-1", className)}>
      <input
        type="text"
        inputMode={field.kind === "int" ? "numeric" : isNumber ? "decimal" : "text"}
        aria-label={label}
        aria-invalid={invalid || undefined}
        disabled={disabled}
        value={edit.draft}
        // Ctrl+Z undoes the edit (not the typing) while the box shows the committed value.
        data-committed={field.value}
        // A locked box leaves the hover text to its row (which says why it is locked).
        title={field.kind === "string" && !disabled ? edit.draft : undefined}
        onChange={(e) => edit.setDraft(e.target.value)}
        onFocus={() => edit.setFocused(true)}
        onBlur={() => {
          edit.setFocused(false);
          edit.commit(edit.draft);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") edit.commit(edit.draft);
          else if (e.key === "Escape") edit.reset();
          else if (isNumber && (e.key === "ArrowUp" || e.key === "ArrowDown")) {
            e.preventDefault();
            edit.step(e.key === "ArrowUp" ? 1 : -1);
          }
        }}
        className={cn(
          "h-6.5 w-full min-w-0 rounded-sm border border-input/70 bg-field px-1.5 text-xs text-foreground outline-none transition-colors focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring",
          isNumber && "text-right font-mono tabular-nums",
          "disabled:cursor-not-allowed disabled:text-muted-foreground",
          inputClassName,
          changed && !invalid && "font-semibold text-changed-foreground",
          invalid &&
            "border-destructive bg-error-container font-semibold text-destructive dark:border-destructive",
        )}
      />
      {steppers && field.kind === "int" ? (
        <div className="flex shrink-0 overflow-hidden rounded-sm bg-surface-highest">
          <button
            type="button"
            aria-label={t("field.decrease")}
            disabled={disabled}
            onClick={() => edit.step(-1)}
            className={STEP}
          >
            <Minus className="size-3" />
          </button>
          <button
            type="button"
            aria-label={t("field.increase")}
            disabled={disabled}
            onClick={() => edit.step(1)}
            className={STEP}
          >
            <Plus className="size-3" />
          </button>
        </div>
      ) : null}
    </div>
  );
}

/** Per-field undo; an empty slot of the same size while the value is unchanged. */
export function UndoButton({ edit }: { edit: FieldEdit }) {
  const { t } = useTranslation("editor");
  if (!edit.changed) return <span className="size-6 shrink-0" aria-hidden="true" />;
  const text = edit.field.added
    ? t("field.removeAdded")
    : t("field.undo", { value: edit.originalText });
  return (
    <button
      type="button"
      disabled={edit.disabled}
      onClick={edit.undo}
      aria-label={text}
      title={text}
      className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-primary outline-none hover:bg-primary/10 focus-visible:ring-2 focus-visible:ring-ring/60 disabled:text-muted-foreground"
    >
      <Undo2 className="size-3.5" />
    </button>
  );
}

/**
 * The value in the file followed by an arrow ("1,250 →"), placed in front of the box that holds
 * the new value, so the pair reads "old → new". Nothing while the value is unchanged.
 */
export function ChangeFrom({ edit, className }: { edit: FieldEdit; className?: string }) {
  const format = useFormatNumber();
  if (!edit.changed) return null;
  const { field } = edit;
  return (
    <span
      className={cn(
        "flex min-w-0 shrink items-center gap-1 font-mono text-[0.6875rem] text-changed-foreground tabular-nums",
        className,
      )}
      title={edit.change ?? undefined}
    >
      <span className="min-w-0 truncate">{changeValue(field.kind, field.original, format)}</span>
      <span aria-hidden="true" className="shrink-0">
        →
      </span>
    </span>
  );
}

/**
 * One value in a dense grid: name (and #id), "old →" when changed, the value box and undo, on one
 * 28 px line. A changed value is tinted violet with a violet bar, a value that cannot be saved is
 * tinted red; the type and limits (or why the value cannot be saved) float under it while it is
 * focused or wrong, so nothing moves.
 */
export function FieldCell({
  field,
  name,
  id,
  nameNode,
  fixedLabel = false,
  title,
  remove,
}: {
  field: Field;
  /** Shown name; also the accessible name of the control. */
  name: string;
  /** Shown as "#12" after the name. */
  id?: number;
  /** Replaces the name text (an own name with its rename button, or its text box). */
  nameNode?: ReactNode;
  /** A short label of fixed width (character stats), so the boxes line up. */
  fixedLabel?: boolean;
  /** Tooltip of the name (defaults to the name). */
  title?: string;
  /** The entry can be removed: show the action (not for a value added in this session). */
  remove?: { path: string; name: string };
}) {
  const { t } = useTranslation("editor");
  const edit = useFieldEdit(field);
  const state = fieldState(edit);
  const kind = edit.field.kind;
  const line = edit.line;
  const floating = line !== null && (edit.focused || edit.invalid) && !line.change ? line : null;

  return (
    <Tooltip open={floating !== null}>
      <TooltipTrigger asChild>
        <div
          data-field-path={field.path}
          className={cn(
            "group relative flex h-7 min-w-0 items-center gap-1 rounded-sm pr-0.5 pl-2 text-xs",
            stateTone(state),
          )}
          // A locked box has no hover text of its own; the row's (which says why) shows over it.
          title={edit.disabled ? (title ?? name) : undefined}
        >
          <StateBar state={state} />
          {nameNode ?? (
            <span
              className={cn(
                "truncate",
                fixedLabel ? "w-[4.5rem] shrink-0 text-muted-foreground" : "min-w-14",
                kind === "string" && !fixedLabel && "max-w-[45%]",
              )}
              title={title ?? name}
            >
              {name}
            </span>
          )}
          {id !== undefined ? (
            <span className="shrink-0 font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
              #{id}
            </span>
          ) : null}
          {edit.field.added ? (
            <span className="shrink-0 rounded-sm bg-changed-chip px-1 text-[0.625rem] font-semibold tracking-wide text-changed-foreground uppercase">
              {t("field.new")}
            </span>
          ) : null}
          {fixedLabel || kind === "string" ? null : (
            <span aria-hidden="true" className="min-w-1 flex-1" />
          )}
          {kind === "bool" || edit.field.added ? null : (
            <ChangeFrom edit={edit} className={fixedLabel ? "max-w-[45%]" : "max-w-[4.5rem]"} />
          )}
          <ValueInput
            edit={edit}
            label={name}
            className={fixedLabel || kind === "string" ? "min-w-12 flex-1 shrink-0" : "min-w-16"}
            // Numbers grow with their digits so a long value is never shown cut short.
            inputClassName={
              fixedLabel || kind === "string"
                ? undefined
                : "w-auto min-w-16 max-w-44 field-sizing-content"
            }
          />
          <UndoButton edit={edit} />
          {remove !== undefined ? (
            <RemoveValueButton path={remove.path} name={remove.name} />
          ) : null}
        </div>
      </TooltipTrigger>
      {floating !== null ? (
        <TooltipContent
          side="bottom"
          align="end"
          sideOffset={2}
          className={cn("max-w-72", floating.error && "font-medium text-destructive")}
        >
          <span role={floating.error ? "alert" : undefined}>{floating.text}</span>
        </TooltipContent>
      ) : null}
    </Tooltip>
  );
}
