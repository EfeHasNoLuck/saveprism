import { ListChecks, ToggleLeft, ToggleRight } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Field } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useEditorContext, useFormatNumber } from "./context";
import { bulkPaths, checkRange, type BulkKind } from "./helpers";

/**
 * "Set all" of a list card: every value the list shows (its search included, on every page) set
 * to one value in one undo step. Switches get "All on" and "All off", the other lists a number box.
 * The buttons count the values that would really change.
 */
export function BulkEdit({
  kind,
  targets,
  searching,
}: {
  kind: BulkKind;
  /** Latest state of the values the list shows that may be set (see `bulkTargets`). */
  targets: Field[];
  searching: boolean;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { editMany } = useEditorContext();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [running, setRunning] = useState(false);

  async function apply(value: string) {
    const paths = bulkPaths(targets, value);
    if (running || paths.length === 0) return;
    setRunning(true);
    const ok = await editMany(paths, value.trim());
    setRunning(false);
    if (ok) setOpen(false);
  }

  const scope = t(searching ? "bulk.scopeSearch" : "bulk.scope", {
    count: targets.length,
    shown: format(targets.length),
  });

  let body: ReactNode;
  if (kind === "bool") {
    const on = bulkPaths(targets, "true").length;
    const off = bulkPaths(targets, "false").length;
    body = (
      <div className="flex gap-2">
        <Button
          size="sm"
          variant="secondary"
          className="flex-1"
          disabled={running || on === 0}
          title={t("bulk.turnOn", { count: on, changed: format(on) })}
          onClick={() => void apply("true")}
        >
          <ToggleRight />
          {t("bulk.allOn")}
          <span className="font-mono text-[0.6875rem] text-muted-foreground tabular-nums">{format(on)}</span>
        </Button>
        <Button
          size="sm"
          variant="secondary"
          className="flex-1"
          disabled={running || off === 0}
          title={t("bulk.turnOff", { count: off, changed: format(off) })}
          onClick={() => void apply("false")}
        >
          <ToggleLeft />
          {t("bulk.allOff")}
          <span className="font-mono text-[0.6875rem] text-muted-foreground tabular-nums">{format(off)}</span>
        </Button>
      </div>
    );
  } else {
    // A whole number unless some value of the list takes decimals; the limits are checked by the host.
    const numberKind = targets.some((f) => f.kind === "float") ? "float" : "int";
    const problem = checkRange(numberKind, text, null, null);
    const changes = problem === null ? bulkPaths(targets, text).length : 0;
    body = (
      <form
        className="flex flex-col gap-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          if (problem === null) void apply(text);
        }}
      >
        <div className="flex gap-2">
          <input
            type="text"
            inputMode={numberKind === "int" ? "numeric" : "decimal"}
            value={text}
            aria-label={t("bulk.valueLabel")}
            placeholder={t("bulk.valueLabel")}
            aria-invalid={(problem !== null && problem !== "empty") || undefined}
            onChange={(event) => setText(event.target.value)}
            className="h-7 w-28 min-w-0 rounded-sm border border-input/70 bg-field px-2 text-right font-mono text-xs tabular-nums outline-none focus-visible:border-ring aria-invalid:border-destructive dark:border-transparent dark:focus-visible:border-ring"
          />
          <Button type="submit" size="sm" className="flex-1" disabled={running || changes === 0}>
            {problem !== null
              ? t("bulk.applyIdle")
              : changes === 0
                ? t("bulk.nothing")
                : t("bulk.apply", { count: changes, changed: format(changes) })}
          </Button>
        </div>
        {problem !== null && problem !== "empty" ? (
          <p role="alert" className="text-xs font-medium text-destructive">
            {t(`problem.${problem}`)}
          </p>
        ) : null}
      </form>
    );
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="text-xs text-foreground-variant"
          disabled={targets.length === 0}
          title={t("bulk.triggerTip")}
        >
          <ListChecks />
          {t("bulk.trigger")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72">
        <p className="text-[0.8125rem] font-semibold">
          {t(kind === "bool" ? "bulk.titleSwitch" : "bulk.titleNumber")}
        </p>
        <p className="mt-0.5 mb-2.5 text-xs text-muted-foreground">{scope}</p>
        {body}
      </PopoverContent>
    </Popover>
  );
}
