import { FileText, FolderOpen, Settings as SettingsIcon } from "lucide-react";
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
  onOpenSettings: () => void;
}

export function Header({ file, onOpenFile, onOpenSettings }: HeaderProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const engine = file ? (engineLabel(file.fileName, file.codec) ?? t("header.engineUnknown")) : null;

  return (
    <header className="relative z-10 flex h-11 shrink-0 items-center justify-between gap-4 border-b border-border bg-chrome px-3">
      <div className="flex min-w-0 items-center gap-2">
        <Logo className="size-5 shrink-0" />
        <span className="text-sm font-semibold tracking-tight">SavePrism</span>
        {file ? (
          <div className="ml-2 flex min-w-0 items-center gap-1.5 rounded-sm border border-border bg-card px-2 py-0.5 dark:border-transparent">
            <FileText className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />
            <span className="truncate font-mono text-xs" title={file.path}>
              {file.fileName}
            </span>
            <span className="shrink-0 rounded-sm bg-surface-high px-1 font-mono text-[11px] leading-4 font-medium text-tertiary uppercase">
              {engine}
            </span>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        <Button
          variant="secondary"
          size="sm"
          onClick={onOpenFile}
          className="mr-1.5 bg-surface-container hover:bg-surface-high"
        >
          <FolderOpen />
          {t("header.openFile")}
        </Button>
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
