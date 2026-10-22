import { useState } from "react";
import { ArrowRight, Braces, ChevronDown, Save, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Change } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { InfoTip } from "./InfoTip";
import type { SaveDefault } from "./settings";

interface BottomBarProps {
  changeCount: number;
  /** The file cannot be saved (guard failed). */
  readOnly: boolean;
  /** The editor reports a value that cannot be saved as typed. */
  invalid: boolean;
  /** A save or another action is running. */
  busy: boolean;
  saveDefault: SaveDefault;
  loadChanges: () => Promise<Change[]>;
  onUndoAll: () => void;
  onOpenJson: () => void;
  onSaveAs: () => void;
  onOverwrite: () => void;
}

export function BottomBar({
  changeCount,
  readOnly,
  invalid,
  busy,
  saveDefault,
  loadChanges,
  onUndoAll,
  onOpenJson,
  onSaveAs,
  onOverwrite,
}: BottomBarProps) {
  const { t } = useTranslation();
  const [changes, setChanges] = useState<Change[] | null>(null);
  const cannotSave = readOnly || invalid || busy;
  const disabledReason = readOnly
    ? t("bottom.readOnlyTip")
    : invalid
      ? t("bottom.invalidTip")
      : undefined;

  const mainAction = saveDefault === "overwrite" ? onOverwrite : onSaveAs;
  const mainLabel = saveDefault === "overwrite" ? t("bottom.saveOverwrite") : t("bottom.saveAs");

  return (
    <footer className="relative z-10 flex h-14 shrink-0 items-center justify-between gap-4 border-t border-border bg-background/90 px-4 backdrop-blur">
      <div className="flex items-center gap-2">
        <Popover
          onOpenChange={(open) => {
            if (!open) return;
            setChanges(null);
            void loadChanges()
              .then(setChanges)
              .catch(() => setChanges([]));
          }}
        >
          <PopoverTrigger asChild>
            <Button variant="ghost" disabled={changeCount === 0}>
              {changeCount === 0
                ? t("bottom.noChanges")
                : t("bottom.changes", { count: changeCount })}
            </Button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-[28rem]">
            <p className="mb-2 text-sm font-medium">{t("bottom.changesTitle")}</p>
            {changes === null ? (
              <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
            ) : (
              <ul className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
                {changes.map((change) => (
                  <li key={change.path} className="rounded-md bg-muted px-3 py-1.5 text-xs">
                    <span className="block truncate font-mono text-muted-foreground" title={change.path}>
                      {change.path}
                    </span>
                    <span className="mt-0.5 flex items-center gap-1.5">
                      <span className="truncate">{change.old}</span>
                      <ArrowRight className="size-3 shrink-0 text-muted-foreground" />
                      <span className="truncate font-medium">{change.new}</span>
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </PopoverContent>
        </Popover>
        <Button variant="ghost" disabled={changeCount === 0 || busy} onClick={onUndoAll}>
          <Undo2 />
          {t("bottom.undoAll")}
        </Button>
        <Button variant="ghost" onClick={onOpenJson}>
          <Braces />
          {t("bottom.openJson")}
        </Button>
        <InfoTip text={t("bottom.openJsonInfo")} />
      </div>

      <div className="flex items-center gap-2" title={disabledReason}>
        <InfoTip text={t("bottom.backupInfo")} />
        <div className="flex">
          <Button
            disabled={cannotSave}
            onClick={mainAction}
            className="rounded-r-none"
          >
            <Save />
            {mainLabel}
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                disabled={cannotSave}
                aria-label={t("bottom.saveOptions")}
                className="rounded-l-none border-l-primary-foreground/30 px-1.5"
              >
                <ChevronDown />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" style={{ width: "auto", minWidth: "16rem" }}>
              <DropdownMenuItem onSelect={onSaveAs}>{t("bottom.saveAsItem")}</DropdownMenuItem>
              <DropdownMenuItem onSelect={onOverwrite}>{t("bottom.overwriteItem")}</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>
    </footer>
  );
}
