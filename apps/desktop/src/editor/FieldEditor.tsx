import { Minus, Plus, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useFormatNumber } from "./context";
import { changeText, changeValue } from "./helpers";
import { BoolChoice } from "./BoolChoice";
import type { FieldEdit } from "./useFieldEdit";

const STEP =
  "inline-flex h-6 w-5 items-center justify-center text-muted-foreground outline-none hover:bg-surface-container hover:text-foreground focus-visible:bg-surface-container focus-visible:text-foreground disabled:pointer-events-none";

/**
 * The value control of a field: a text box (with − and + for whole numbers) or the `true | false`
 * choice. A changed value is violet, a value that cannot be saved is red.
 */
export function ValueInput({
  edit,
  label,
  inputClassName,
  className,
}: {
  edit: FieldEdit;
  /** Accessible name of the control. */
  label: string;
  /** Extra classes of the text box (width, accent colour). */
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
        onChange={(e) => edit.setDraft(e.target.value)}
        onFocus={() => edit.setFocused(true)}
        onBlur={() => {
          edit.setFocused(false);
          edit.commit(edit.draft);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") edit.commit(edit.draft);
          else if (e.key === "Escape") edit.reset();
        }}
        className={cn(
          "h-6 w-full min-w-0 rounded-sm border border-input/70 bg-field px-1.5 text-[13px] text-foreground outline-none transition-colors focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring",
          isNumber && "font-mono text-xs tabular-nums",
          "disabled:cursor-not-allowed disabled:text-muted-foreground",
          inputClassName,
          changed && !invalid && "font-semibold text-changed-foreground",
          invalid &&
            "border-destructive bg-error-container font-semibold text-destructive dark:border-destructive",
        )}
      />
      {field.kind === "int" ? (
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
  const text = t("field.undo", { value: edit.originalText });
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

/** "old → new" with the new value on a violet chip. */
export function ChangeChip({ edit }: { edit: FieldEdit }) {
  const format = useFormatNumber();
  const { field } = edit;
  return (
    <span
      className="inline-flex min-w-0 items-center gap-1 font-mono text-xs text-changed-foreground tabular-nums"
      title={changeText(field.kind, field.original, field.value, format, Infinity)}
    >
      <span className="min-w-0 truncate">{changeValue(field.kind, field.original, format)}</span>
      <span aria-hidden="true" className="shrink-0 text-muted-foreground">
        →
      </span>
      <span className="min-w-0 truncate rounded-sm bg-changed-chip px-1 font-medium">
        {changeValue(field.kind, field.value, format)}
      </span>
    </span>
  );
}

/**
 * The Change column: why the value cannot be saved, the type and limits while the box is focused,
 * "old → new" when changed, else an em dash.
 */
export function ChangeCell({ edit }: { edit: FieldEdit }) {
  const line = edit.line;
  if (line?.error) {
    return (
      <span role="alert" title={line.text} className="truncate text-xs font-medium text-destructive">
        {line.text}
      </span>
    );
  }
  if (line !== null && !line.change) {
    return (
      <span title={line.text} className="truncate text-xs text-muted-foreground">
        {line.text}
      </span>
    );
  }
  if (edit.changed) return <ChangeChip edit={edit} />;
  return (
    <span aria-hidden="true" className="text-muted-foreground">
      —
    </span>
  );
}

/**
 * A field in a tight spot (roster cells, Gold): the value control and its undo. The hint (type and
 * limits, or the error) floats under it while it is focused or wrong, so nothing moves.
 */
export function InlineField({
  edit,
  label,
  inputClassName,
  showChange = false,
  className,
}: {
  edit: FieldEdit;
  label: string;
  inputClassName?: string;
  /** Show the "old → new" chip after the control (Gold). */
  showChange?: boolean;
  className?: string;
}) {
  const line = edit.line;
  const floating = line !== null && (edit.focused || edit.invalid) && !line.change ? line : null;
  return (
    <Tooltip open={floating !== null}>
      <TooltipTrigger asChild>
        <div
          className={cn("flex min-w-0 items-center gap-1", className)}
          title={edit.changed && !showChange && edit.change !== null ? edit.change : undefined}
        >
          <ValueInput edit={edit} label={label} inputClassName={inputClassName} />
          {showChange && edit.changed ? <ChangeChip edit={edit} /> : null}
          <UndoButton edit={edit} />
        </div>
      </TooltipTrigger>
      {floating !== null ? (
        <TooltipContent
          side="bottom"
          align="start"
          sideOffset={4}
          className={cn("max-w-72", floating.error && "text-destructive")}
        >
          <span role={floating.error ? "alert" : undefined}>{floating.text}</span>
        </TooltipContent>
      ) : null}
    </Tooltip>
  );
}
