/**
 * Applies the settings that change how the whole app looks: the root font size, the app colour and
 * the colour presets. `main.tsx` calls `applyStoredAppearance()` before the first render, like the
 * theme.
 */

import { appColorCss, type AppColor } from "./appColor";
import { colorCss, type ColorChoices } from "./colors";
import { browserStorage, loadSettings, textScalePx, type Settings } from "./settings";

const COLOR_STYLE_ID = "saveprism-colors";
const APP_COLOR_STYLE_ID = "saveprism-appcolor";

/** Sets the root font size; the rem-based Tailwind sizes scale with it. */
export function applyTextScale(scale: number): void {
  document.documentElement.style.fontSize = `${textScalePx(scale)}px`;
}

/** Writes the chosen colours (presets or custom) over the CSS variables for both themes. */
export function applyColors(colors: ColorChoices): void {
  let style = document.getElementById(COLOR_STYLE_ID) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = COLOR_STYLE_ID;
    document.head.appendChild(style);
  }
  style.textContent = colorCss(colors);
}

/**
 * Writes the app-colour palette over the surfaces and action tokens of both themes. The blue
 * default writes nothing. `accentHex` is the Windows accent while the system choice is active.
 */
export function applyAppColor(choice: AppColor, accentHex?: string | null): void {
  let style = document.getElementById(APP_COLOR_STYLE_ID) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = APP_COLOR_STYLE_ID;
    document.head.appendChild(style);
  }
  style.textContent = appColorCss(choice, accentHex);
}

const ACCENT_KEY = "saveprism.windowsAccent";

/** The Windows accent read on the last run, so the system choice paints before the host answers. */
export function storedAccent(): string | null {
  try {
    return browserStorage()?.getItem(ACCENT_KEY) ?? null;
  } catch {
    return null;
  }
}

/** Keeps the Windows accent for the next start; null forgets it. */
export function rememberAccent(hex: string | null): void {
  try {
    const storage = browserStorage();
    if (hex) storage?.setItem(ACCENT_KEY, hex);
    else storage?.removeItem(ACCENT_KEY);
  } catch {
    // Storage may be blocked; the accent is read again at the next start.
  }
}

/** Applies the stored text size and colours synchronously, before the first paint. */
export function applyStoredAppearance(): void {
  const settings: Settings = loadSettings(browserStorage());
  applyTextScale(settings.textScale);
  applyAppColor(settings.appColor, storedAccent());
  applyColors(settings.colors);
}
