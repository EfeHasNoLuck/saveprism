import { Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { FieldKind } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useEditorContext } from "./context";
import { checkRange } from "./helpers";

type NewKind = "int" | "string" | "bool";

const KINDS: NewKind[] = ["int", "string", "bool"];

/** "Add value": a key, a type (number, text, on/off) and its first value. */
export function AddValueButton({
  parent,
  compact = false,
}: {
  /** Path of the object (or story variables) the new value goes into. */
  parent: string;
  /** A small icon button for a group heading instead of the card's labelled button. */
  compact?: boolean;
}) {
  const { t } = useTranslation("editor");
  const { addValue } = useEditorContext();
  const [open, setOpen] = useState(false);
  const [key, setKey] = useState("");
  const [kind, setKind] = useState<NewKind>("int");
  const [text, setText] = useState("0");
  const [running, setRunning] = useState(false);

  const problem = kind === "int" ? checkRange("int", text, null, null) : null;
  const valid = key.trim() !== "" && problem === null;

  function changeKind(next: NewKind) {
    setKind(next);
    setText(next === "bool" ? "false" : next === "int" ? "0" : "");
  }

  async function submit() {
    if (!valid || running) return;
    setRunning(true);
    const ok = await addValue(parent, key.trim(), kind as FieldKind, text);
    setRunning(false);
    if (ok) {
      setOpen(false);
      setKey("");
      setKind("int");
      setText("0");
    }
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size={compact ? "icon-xs" : "sm"}
          className={compact ? "text-muted-foreground" : "text-xs text-foreground-variant"}
          aria-label={t("addValue.action")}
          title={t("addValue.action")}
        >
          <Plus />
          {compact ? null : t("addValue.action")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72">
        <p className="text-[13px] font-semibold">{t("addValue.title")}</p>
        <p className="mt-0.5 mb-2.5 text-xs text-muted-foreground">{t("addValue.hint")}</p>
        <form
          className="flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          <label className="flex flex-col gap-1 text-xs text-muted-foreground">
            {t("addValue.key")}
            <input
              type="text"
              value={key}
              autoFocus
              aria-label={t("addValue.key")}
              placeholder={t("addValue.keyPlaceholder")}
              onChange={(event) => setKey(event.target.value)}
              className="h-7 w-full rounded-sm border border-input/70 bg-field px-2 text-xs text-foreground outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
            />
          </label>
          <div className="flex gap-2">
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
              {t("addValue.type")}
              <select
                value={kind}
                aria-label={t("addValue.type")}
                onChange={(event) => changeKind(event.target.value as NewKind)}
                className="h-7 w-full rounded-sm border border-input/70 bg-field px-1.5 text-xs text-foreground outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
              >
                {KINDS.map((option) => (
                  <option key={option} value={option}>
                    {t(`addValue.kind.${option}`)}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex min-w-0 flex-1 flex-col gap-1 text-xs text-muted-foreground">
              {t("addValue.value")}
              {kind === "bool" ? (
                <select
                  value={text}
                  aria-label={t("addValue.value")}
                  onChange={(event) => setText(event.target.value)}
                  className="h-7 w-full rounded-sm border border-input/70 bg-field px-1.5 text-xs text-foreground outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
                >
                  <option value="false">false</option>
                  <option value="true">true</option>
                </select>
              ) : (
                <input
                  type="text"
                  inputMode={kind === "int" ? "numeric" : "text"}
                  value={text}
                  aria-label={t("addValue.value")}
                  aria-invalid={problem !== null || undefined}
                  onChange={(event) => setText(event.target.value)}
                  className="h-7 w-full rounded-sm border border-input/70 bg-field px-2 text-right font-mono text-xs tabular-nums outline-none focus-visible:border-ring aria-invalid:border-destructive dark:border-transparent dark:focus-visible:border-ring"
                />
              )}
            </label>
          </div>
          {problem !== null && problem !== "empty" ? (
            <p role="alert" className="text-xs font-medium text-destructive">
              {t(`problem.${problem}`)}
            </p>
          ) : null}
          <Button type="submit" size="sm" disabled={!valid || running}>
            <Plus />
            {t("addValue.submit")}
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
