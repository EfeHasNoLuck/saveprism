import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Info, Plus, RotateCcw, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useSettings } from "./context";
import {
  combinationFromInput,
  conflictingAction,
  formatCombination,
  MAX_SHORTCUTS_PER_ACTION,
  removeShortcut,
  resetAllShortcuts,
  resetShortcut,
  SHORTCUT_ACTIONS,
  validateCombination,
  withShortcut,
  type CombinationProblem,
  type ParsedCombination,
  type ShortcutAction,
  type ShortcutBindings,
  type ShortcutGroup,
} from "./shortcuts";

interface Recording {
  action: ShortcutAction;
  /** The slot being replaced; undefined when a new shortcut is added. */
  replace?: number;
}

interface Pressed {
  combination: ParsedCombination;
  problem: CombinationProblem | null;
  conflict: ShortcutAction | null;
}

const GROUPS: ShortcutGroup[] = ["file", "editing", "app"];

function KeyCaps({ combination, large }: { combination: string; large?: boolean }) {
  return (
    <span className="flex items-center gap-1">
      {combination.split("+").map((part) => (
        <kbd
          key={part}
          className={
            large
              ? "inline-flex min-w-10 items-center justify-center rounded-md border border-border bg-field px-2.5 py-1.5 font-mono text-sm font-medium text-foreground shadow-sm"
              : "rounded-sm bg-surface-high px-1.5 py-0.5 font-mono text-[0.6875rem] text-foreground-variant"
          }
        >
          {part}
        </kbd>
      ))}
    </span>
  );
}

/**
 * The shortcuts tab of Settings. Rows list the bindings of each action; clicking one (or "+")
 * opens the recorder, and the choices are stored with the rest of the settings.
 */
export function ShortcutSettings() {
  const { t } = useTranslation();
  const { settings, update } = useSettings();
  const bindings = settings.shortcuts;
  const [recording, setRecording] = useState<Recording | null>(null);
  const [pressed, setPressed] = useState<Pressed | null>(null);
  const [heldCaps, setHeldCaps] = useState<string[]>([]);
  const pressedRef = useRef<Pressed | null>(null);

  const save = useCallback(
    (next: ShortcutBindings) => update({ shortcuts: next }),
    [update],
  );

  const start = useCallback((action: ShortcutAction, replace?: number) => {
    setPressed(null);
    pressedRef.current = null;
    setHeldCaps([]);
    setRecording({ action, replace });
  }, []);

  const cancel = useCallback(() => {
    setRecording(null);
    setPressed(null);
    pressedRef.current = null;
    setHeldCaps([]);
  }, []);

  const commit = useCallback(
    (value: ParsedCombination) => {
      if (!recording) return;
      const combination = formatCombination(value);
      const next = withShortcut(bindings, recording.action, combination, recording.replace);
      save(next);
      cancel();
    },
    [bindings, cancel, recording, save],
  );

  useEffect(() => {
    if (!recording) return;
    const listener = (event: KeyboardEvent) => {
      // The dialog owns every key while it records: no app shortcut, no Settings Esc.
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        cancel();
        return;
      }
      if (event.key === "Enter") {
        const current = pressedRef.current;
        if (current !== null && current.problem === null) commit(current.combination);
        return;
      }

      const combination = combinationFromInput(event);
      if (combination === null) {
        setHeldCaps(modifierCaps(event));
        return;
      }
      const problem = validateCombination(combination);
      const conflict =
        problem === null
          ? conflictingAction(bindings, recording.action, formatCombination(combination))
          : null;
      const next: Pressed = { combination, problem, conflict };
      pressedRef.current = next;
      setPressed(next);
      setHeldCaps(formatCombination(combination).split("+"));
    };
    window.addEventListener("keydown", listener, true);
    return () => window.removeEventListener("keydown", listener, true);
  }, [bindings, cancel, commit, recording]);

  const groups = useMemo(
    () =>
      GROUPS.map((group) => ({
        group,
        actions: SHORTCUT_ACTIONS.filter((entry) => entry.group === group).map(
          (entry) => entry.action,
        ),
      })),
    [],
  );

  const problemMessage = useCallback(
    (value: Pressed): string | null => {
      if (value.problem !== null) {
        const key = `settings.shortcutDialog.${value.problem}`;
        return t(key, { keys: formatCombination(value.combination) });
      }
      if (value.conflict !== null) {
        return t("settings.shortcutDialog.conflict", {
          keys: formatCombination(value.combination),
          action: t(`shortcuts.${value.conflict}`),
        });
      }
      return null;
    },
    [t],
  );

  const actionName = recording ? t(`shortcuts.${recording.action}`) : "";
  const caps = pressed !== null ? formatCombination(pressed.combination).split("+") : heldCaps;
  const message = recording && pressed !== null ? problemMessage(pressed) : null;

  return (
    <div className="mt-4">
      <div className="mb-2 flex justify-end">
        <Button
          variant="ghost"
          size="xs"
          className="text-foreground-variant"
          onClick={() => save(resetAllShortcuts())}
        >
          <RotateCcw className="size-3" />
          {t("settings.resetAllShortcuts")}
        </Button>
      </div>

      <div className="flex flex-col gap-5">
        {groups.map(({ group, actions }) => (
          <section key={group}>
            <h3 className="mb-1.5 text-[0.6875rem] font-semibold tracking-[0.06em] text-muted-foreground uppercase">
              {t(`shortcuts.groups.${group}`)}
            </h3>
            <div className="overflow-hidden rounded-lg border border-border bg-card dark:border-[#1d2d42]">
              {actions.map((action, index) => {
                const keys = bindings[action];
                return (
                  <div
                    key={action}
                    data-setting={`shortcuts.${action}`}
                    className={`flex min-h-12 items-center gap-4 px-4 py-2 text-[0.8125rem] ${
                      index === 0 ? "" : "border-t border-border dark:border-[#1d2d42]"
                    }`}
                  >
                    <span className="w-52 shrink-0 font-medium">{t(`shortcuts.${action}`)}</span>
                    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
                      {keys.length === 0 ? (
                        <span className="text-xs text-muted-foreground">
                          {t("settings.shortcutNotSet")}
                        </span>
                      ) : null}
                      {keys.map((key, slot) => (
                        <span
                          key={key}
                          className="group/chip inline-flex items-center gap-0.5 rounded-sm border border-border bg-surface-container pr-0.5 pl-1"
                        >
                          <button
                            type="button"
                            onClick={() => start(action, slot)}
                            title={t("settings.editShortcut")}
                            className="rounded-sm py-0.5 outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                          >
                            <KeyCaps combination={key} />
                          </button>
                          <button
                            type="button"
                            aria-label={t("settings.removeShortcut", { keys: key })}
                            title={t("settings.removeShortcut", { keys: key })}
                            onClick={() => save(removeShortcut(bindings, action, key))}
                            className="rounded-sm p-0.5 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
                          >
                            <X className="size-3" />
                          </button>
                        </span>
                      ))}
                      {keys.length < MAX_SHORTCUTS_PER_ACTION ? (
                        <button
                          type="button"
                          aria-label={t("settings.addShortcut")}
                          title={t("settings.addShortcut")}
                          onClick={() => start(action)}
                          className="inline-flex size-6 items-center justify-center rounded-sm border border-border border-dashed text-muted-foreground outline-none hover:border-primary hover:text-primary focus-visible:ring-2 focus-visible:ring-ring/60"
                        >
                          <Plus className="size-3.5" />
                        </button>
                      ) : null}
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        className="ml-1 text-muted-foreground"
                        aria-label={t("settings.resetShortcut", { action: t(`shortcuts.${action}`) })}
                        title={t("settings.resetShortcut", { action: t(`shortcuts.${action}`) })}
                        onClick={() => save(resetShortcut(bindings, action))}
                      >
                        <RotateCcw />
                      </Button>
                    </div>
                  </div>
                );
              })}
            </div>
          </section>
        ))}
      </div>

      <div className="mt-3 flex items-start gap-2.5 rounded-lg border border-border bg-primary/7 px-4 py-2.5 text-xs leading-[17px] text-foreground dark:border-[#1d2d42]">
        <Info className="mt-px size-4 shrink-0 text-primary" />
        <span>{t("settings.shortcutsNote")}</span>
      </div>

      <Dialog open={recording !== null} onOpenChange={(open) => (open ? undefined : cancel())}>
        <DialogContent showClose={false} className="max-w-sm">
          <DialogHeader className="items-center gap-1 pr-0 text-center">
            <DialogTitle className="text-sm">
              {t("settings.shortcutDialog.title", { action: actionName })}
            </DialogTitle>
            <DialogDescription>{t("settings.shortcutDialog.hint")}</DialogDescription>
          </DialogHeader>
          <div className="flex min-h-14 flex-wrap items-center justify-center gap-1.5 py-1">
            {caps.length === 0 ? (
              <span className="text-xs text-muted-foreground">
                {t("settings.shortcutDialog.waiting")}
              </span>
            ) : (
              <KeyCaps combination={caps.join("+")} large />
            )}
          </div>
          {message !== null ? (
            <p role="alert" className="text-center text-xs leading-[17px] text-warning">
              {message}
            </p>
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}

function modifierCaps(event: KeyboardEvent): string[] {
  const caps: string[] = [];
  if (event.ctrlKey || event.metaKey) caps.push("Ctrl");
  if (event.shiftKey) caps.push("Shift");
  if (event.altKey) caps.push("Alt");
  return caps;
}
