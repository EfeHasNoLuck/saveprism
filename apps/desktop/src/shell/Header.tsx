import {
  ClipboardPaste,
  FileText,
  FolderOpen,
  FolderSearch,
  Settings as SettingsIcon,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { OpenInfo } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { LanguageSwitcher } from "@/i18n";
import { ThemeToggle } from "@/theme";
import { Bell } from "./Bell";
import { useSettings } from "./context";
import { engineLabel } from "./engine";
import { Logo } from "./Logo";

interface HeaderProps {
  file: OpenInfo | null;
  onOpenFile: () => void;
  /** Opens the dialog that takes a pasted save code. */
  onPasteCode: () => void;
  /** Closes the open file (or pasted code) and goes back to the start screen. */
  onCloseFile: () => void;
  /** Opens the dialog that lists the usual save folders of each engine. */
  onWhereSaves: () => void;
  onOpenSettings: () => void;
}

/**
 * The top bar. With no file open it offers Paste code and Open file; while a file is open, one
 * Close file button takes their place (the start screen offers them again).
 */
export function Header({
  file,
  onOpenFile,
  onPasteCode,
  onCloseFile,
  onWhereSaves,
  onOpenSettings,
}: HeaderProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const known = file ? engineLabel(file.fileName, file.codec, file.engine) : null;
  const engine = file ? (known ?? t("header.engineUnknown")) : null;

  return (
    <header className="relative z-10 flex h-11 shrink-0 items-center justify-between gap-4 border-b border-border bg-chrome px-3">
      <div className="flex min-w-0 items-center gap-2">
        <Logo className="size-5 shrink-0" />
        <span className="text-sm font-semibold tracking-tight">SavePrism</span>
        {file ? (
          <div className="ml-2 flex min-w-0 items-center gap-1.5 rounded-sm border border-border bg-card px-2 py-0.5 dark:border-transparent">
            {file.pasted ? (
              <>
                <ClipboardPaste className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
                <span className="truncate text-xs font-medium" title={t("header.pastedTip")}>
                  {t("header.pastedCode")}
                </span>
              </>
            ) : (
              <>
                <FileText className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
                <span className="truncate font-mono text-xs" title={file.path}>
                  {file.fileName}
                </span>
              </>
            )}
            {/* Engine names are English: uppercase them by English rules ("SQLITE", not "SQLİTE"). */}
            <span
              lang={known ? "en" : undefined}
              className="shrink-0 rounded-sm bg-surface-high px-1 font-mono text-[0.6875rem] leading-4 font-medium text-tertiary uppercase"
            >
              {engine}
            </span>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        {file ? (
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={onCloseFile}
              title={t("header.closeFileTip")}
              className="mr-1 bg-surface-container hover:bg-surface-high"
            >
              <X />
              {t("header.closeFile")}
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={t("start.whereLink")}
              title={t("start.whereLink")}
              onClick={onWhereSaves}
              className="mr-1.5 text-foreground-variant"
            >
              <FolderSearch />
            </Button>
          </>
        ) : (
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={onPasteCode}
              title={t("header.pasteCodeTip")}
              className="mr-1 bg-surface-container hover:bg-surface-high"
            >
              <ClipboardPaste />
              {t("header.pasteCode")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={onOpenFile}
              className="mr-1.5 bg-surface-container hover:bg-surface-high"
            >
              <FolderOpen />
              {t("header.openFile")}
            </Button>
          </>
        )}
        <LanguageSwitcher />
        <ThemeToggle />
        {settings.showNotifications ? <Bell /> : null}
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("header.settings")}
          title={t("header.settings")}
          onClick={onOpenSettings}
          className="text-foreground-variant"
        >
          <SettingsIcon />
        </Button>
      </div>
    </header>
  );
}
