import { useCallback, useEffect, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { appAccentColor, shareSettings } from "@/api/commands";
import {
  applyAppColor,
  applyColors,
  applyTextScale,
  rememberAccent,
  storedAccent,
} from "./appearance";
import { SettingsContext, type SettingsContextValue } from "./context";
import {
  browserStorage,
  loadSettings,
  saveSettings,
  type Settings,
} from "./settings";

/** Holds the user's settings and stores them locally on every change. */
export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings>(() => loadSettings(browserStorage()));
  const [accentHex, setAccentHex] = useState<string | null>(storedAccent);

  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch };
      saveSettings(next, browserStorage());
      return next;
    });
  }, []);

  // The Windows accent is read when the system choice becomes active and again whenever the
  // window gets focus, so a change in Windows shows up without a restart.
  useEffect(() => {
    if (settings.appColor.kind !== "system") {
      return;
    }
    let alive = true;
    const read = () => {
      appAccentColor()
        .then((value) => {
          rememberAccent(value);
          if (alive) setAccentHex(value);
        })
        .catch(() => {
          if (alive) setAccentHex(null);
        });
    };
    read();
    window.addEventListener("focus", read);
    return () => {
      alive = false;
      window.removeEventListener("focus", read);
    };
  }, [settings.appColor.kind]);

  // The MCP server cannot read this storage, so the folders and backup counts it needs go to a
  // small shared file. A failure stays quiet: the MCP server then uses the default folders.
  const { backupFolders, appFolder, keepBackups, keepBackupDays } = settings;
  useEffect(() => {
    shareSettings({ backupFolders, appFolder, keepBackups, keepBackupDays }).catch(() => undefined);
  }, [backupFolders, appFolder, keepBackups, keepBackupDays]);

  useLayoutEffect(() => {
    applyTextScale(settings.textScale);
    applyAppColor(settings.appColor, accentHex);
    applyColors(settings.colors);
  }, [settings.textScale, settings.appColor, settings.colors, accentHex]);

  const value = useMemo<SettingsContextValue>(() => ({ settings, update }), [settings, update]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}
