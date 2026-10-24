/**
 * Applies the settings that change how the whole app looks: the root font size and the colour
 * presets. `main.tsx` calls `applyStoredAppearance()` before the first render, like the theme.
 */

import { colorCss, type ColorChoices } from "./colors";
import {
  browserStorage,
  loadSettings,
  TEXT_SIZE_PX,
  type Settings,
  type TextSize,
} from "./settings";

const COLOR_STYLE_ID = "saveprism-colors";

/** Sets the root font size; the rem-based Tailwind sizes scale with it. */
export function applyTextSize(size: TextSize): void {
  document.documentElement.style.fontSize = `${TEXT_SIZE_PX[size]}px`;
}

/** Writes the chosen presets over the CSS variables for both themes. */
export function applyColors(colors: ColorChoices): void {
  let style = document.getElementById(COLOR_STYLE_ID) as HTMLStyleElement | null;
  if (!style) {
    style = document.createElement("style");
    style.id = COLOR_STYLE_ID;
    document.head.appendChild(style);
  }
  style.textContent = colorCss(colors);
}

/** Applies the stored text size and colours synchronously, before the first paint. */
export function applyStoredAppearance(): void {
  const settings: Settings = loadSettings(browserStorage());
  applyTextSize(settings.textSize);
  applyColors(settings.colors);
}
