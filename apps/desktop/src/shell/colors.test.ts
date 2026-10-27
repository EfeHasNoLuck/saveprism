// @vitest-environment node
import { describe, expect, it } from "vitest";
import { COLOR_GROUPS, COLOR_PRESETS, colorCss, DEFAULT_COLORS } from "./colors";

/** The card background and the muted label colour of each theme (`index.css`). */
const CARD = { light: "#ffffff", dark: "#0b1c30" };
const MUTED = { light: "#4f5563", dark: "#a0a6b4" };
/** How strongly a changed / invalid row is tinted (`--changed-row`, `--error-row`). */
const ROW_TINT = { changed: { light: 0.09, dark: 0.2 }, error: { light: 0.08, dark: 0.3 } };

/** WCAG relative luminance of a `#rrggbb` colour. */
function luminance(hex: string): number {
  const value = Number.parseInt(hex.slice(1), 16);
  const channels = [value >> 16, (value >> 8) & 0xff, value & 0xff].map((byte) => {
    const c = byte / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

/** WCAG contrast ratio of two `#rrggbb` colours. */
function contrast(a: string, b: string): number {
  const [brighter, darker] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (brighter + 0.05) / (darker + 0.05);
}

/** Hue of a `#rrggbb` colour in degrees (0–360). */
function hue(hex: string): number {
  const value = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [value >> 16, (value >> 8) & 0xff, value & 0xff].map((byte) => byte / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  if (delta === 0) return 0;
  const sector =
    max === r ? ((g - b) / delta) % 6 : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4;
  return (sector * 60 + 360) % 360;
}

/** The smaller distance between two hues around the colour wheel. */
function hueDistance(a: number, b: number): number {
  const distance = Math.abs(a - b) % 360;
  return Math.min(distance, 360 - distance);
}

/** `fg` laid over `bg` at `alpha`, as `color-mix(in srgb, fg alpha, transparent)` on `bg` looks. */
function over(fg: string, alpha: number, bg: string): string {
  const parse = (hex: string) => {
    const value = Number.parseInt(hex.slice(1), 16);
    return [value >> 16, (value >> 8) & 0xff, value & 0xff];
  };
  const [f, b] = [parse(fg), parse(bg)];
  const mixed = f.map((channel, i) => Math.round(alpha * channel + (1 - alpha) * b[i]));
  return `#${mixed.map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

describe("colour presets", () => {
  it("starts each group at the colour the app already uses", () => {
    expect(COLOR_PRESETS.changed[0]).toMatchObject({
      id: "violet",
      light: { main: "#6b38d4", container: "#e9ddff", onContainer: "#23005c" },
      dark: { main: "#d0bcff", container: "#571bc1", onContainer: "#e9ddff" },
    });
    expect(COLOR_PRESETS.accent[0]).toMatchObject({
      id: "cyan",
      light: { main: "#00687a", container: "#acedff", onContainer: "#001f26" },
      dark: { main: "#4cd7f6", container: "#009eb9", onContainer: "#002f38" },
    });
    expect(COLOR_PRESETS.error[0]).toMatchObject({
      id: "red",
      light: { main: "#ba1a1a", container: "#ffdad6", onContainer: "#410002" },
      dark: { main: "#ffb4ab", container: "#93000a", onContainer: "#ffdad6" },
    });
  });

  it("keeps every preset's text readable on the card, its container and its tinted row", () => {
    for (const group of COLOR_GROUPS) {
      for (const preset of COLOR_PRESETS[group]) {
        for (const theme of ["light", "dark"] as const) {
          const { main, container, onContainer } = preset[theme];
          const at = `${group}/${preset.id} ${theme}`;
          expect(contrast(main, CARD[theme]), `${at} text`).toBeGreaterThanOrEqual(4.5);
          expect(contrast(onContainer, container), `${at} container`).toBeGreaterThanOrEqual(4.5);
          if (group !== "accent") {
            const tint = ROW_TINT[group][theme];
            const row = over(theme === "dark" ? container : main, tint, CARD[theme]);
            expect(contrast(MUTED[theme], row), `${at} row label`).toBeGreaterThanOrEqual(4.5);
            expect(contrast(main, row), `${at} row text`).toBeGreaterThanOrEqual(4.5);
          }
        }
      }
    }
  });

  it("keeps every error preset on a clearly different hue", () => {
    for (const theme of ["light", "dark"] as const) {
      const hues = COLOR_PRESETS.error.map((preset) => hue(preset[theme].main));
      for (let i = 0; i < hues.length; i += 1) {
        for (let j = i + 1; j < hues.length; j += 1) {
          expect(
            hueDistance(hues[i], hues[j]),
            `error/${COLOR_PRESETS.error[i].id} vs error/${COLOR_PRESETS.error[j].id} ${theme}`,
          ).toBeGreaterThanOrEqual(15);
        }
      }
    }
  });

  it("writes no CSS for the default choices and both theme blocks otherwise", () => {
    expect(colorCss(DEFAULT_COLORS)).toBe("");
    const css = colorCss({ ...DEFAULT_COLORS, changed: "rose", accent: "green" });
    expect(css).toContain(":root {");
    expect(css).toContain(".dark {");
    expect(css).toContain("--changed: #b01a75;");
    expect(css).toContain("--changed: #ffafd5;");
    expect(css).toContain("--changed-tag: #8b015a;");
    expect(css).toContain("--tertiary: #146c45;");
    expect(css).not.toContain("--destructive:");
  });
});
