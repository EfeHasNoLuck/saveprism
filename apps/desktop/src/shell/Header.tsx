import { FolderOpen, Settings as SettingsIcon } from "lucide-react";
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
    <header className="relative z-10 flex h-14 shrink-0 items-center justify-between gap-4 border-b border-border bg-background/80 px-4 backdrop-blur">
      <div className="flex min-w-0 items-center gap-3">
        <Logo className="size-7 shrink-0" />
        <span className="text-base font-semibold">SavePrism</span>
        {file ? (
          <div className="ml-2 flex min-w-0 items-center gap-2 border-l border-border pl-4">
            <span className="truncate text-sm" title={file.path}>
              {file.fileName}
            </span>
            <span className="shrink-0 rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
              {engine}
            </span>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button onClick={onOpenFile} className="mr-2">
          <FolderOpen />
          {t("header.openFile")}
        </Button>
        <LanguageSwitcher />
        <ThemeToggle />
        {settings.showNotifications ? <Bell /> : null}
        <Button
          variant="ghost"
          size="icon"
          aria-label={t("header.settings")}
          onClick={onOpenSettings}
        >
          <SettingsIcon />
        </Button>
      </div>
    </header>
  );
}
