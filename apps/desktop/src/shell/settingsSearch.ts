/** The searchable rows of the Settings page and their per-language search. No React here. */

export type SettingsTab = "general" | "appearance" | "backups" | "ai" | "shortcuts" | "about";

export interface SettingEntry {
  tab: SettingsTab;
  /** Stable id of the row, used to highlight it after a jump. */
  id: string;
  /** Translation key of the row's label. */
  labelKey: string;
}

/** Every row "Find a setting" can jump to. */
export const SETTINGS_INDEX: readonly SettingEntry[] = [
  { tab: "general", id: "general.language", labelKey: "settings.language" },
  { tab: "general", id: "general.theme", labelKey: "settings.theme" },
  { tab: "general", id: "general.notifications", labelKey: "settings.showNotifications" },
  { tab: "appearance", id: "appearance.textSize", labelKey: "settings.textSize" },
  { tab: "appearance", id: "appearance.changed", labelKey: "settings.changedColor" },
  { tab: "appearance", id: "appearance.accent", labelKey: "settings.accentColor" },
  { tab: "appearance", id: "appearance.error", labelKey: "settings.errorColor" },
  { tab: "appearance", id: "appearance.background", labelKey: "settings.background" },
  { tab: "appearance", id: "appearance.strength", labelKey: "settings.patternStrength" },
  { tab: "appearance", id: "appearance.photo", labelKey: "settings.photo" },
  { tab: "appearance", id: "appearance.fit", labelKey: "settings.photoFit" },
  { tab: "appearance", id: "appearance.blur", labelKey: "settings.blur" },
  { tab: "appearance", id: "appearance.darken", labelKey: "settings.darken" },
  { tab: "backups", id: "backups.action", labelKey: "settings.defaultAction" },
  { tab: "backups", id: "backups.keep", labelKey: "settings.keepBackups" },
  { tab: "backups", id: "backups.days", labelKey: "settings.keepDays" },
  { tab: "backups", id: "backups.folder", labelKey: "settings.backupFolder" },
  { tab: "backups", id: "backups.appFolder", labelKey: "settings.appFolder" },
];

/**
 * The rows whose label in the current language holds `query` (case-insensitive, trimmed). An empty
 * query finds nothing.
 */
export function searchSettings(
  query: string,
  translate: (key: string) => string,
  locale?: string,
): SettingEntry[] {
  const needle = query.trim().toLocaleLowerCase(locale);
  if (needle === "") return [];
  return SETTINGS_INDEX.filter((entry) =>
    translate(entry.labelKey).toLocaleLowerCase(locale).includes(needle),
  );
}
