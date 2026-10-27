import { useCallback, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { applyColors, applyTextScale } from "./appearance";
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

  const update = useCallback((patch: Partial<Settings>) => {
    setSettings((current) => {
      const next = { ...current, ...patch };
      saveSettings(next, browserStorage());
      return next;
    });
  }, []);

  useLayoutEffect(() => {
    applyTextScale(settings.textScale);
    applyColors(settings.colors);
  }, [settings.textScale, settings.colors]);

  const value = useMemo<SettingsContextValue>(() => ({ settings, update }), [settings, update]);
  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}
