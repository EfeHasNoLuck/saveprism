/**
 * The app-wide colour: a hue that tints every surface and action token of the theme. Pure logic
 * (no DOM) so the palette and its contrast can be tested in a node environment.
 *
 * The lightness of every token is the one `index.css` gives the blue theme; only the hue and the
 * chroma are replaced, so every choice keeps the structure and contrast of the default. Work is
 * done in OKLCH, where an equal lightness step looks equally heavy for every hue.
 */

import { contrastRatio, isHexColor } from "./colors";

/** The tokens the app-colour palette writes (the changed / accent / error groups stay theirs). */
export const APP_COLOR_TOKENS = [
  "brand",
  "background",
  "foreground",
  "foreground-variant",
  "chrome",
  "card",
  "card-foreground",
  "popover",
  "popover-foreground",
  "surface-container",
  "surface-high",
  "surface-highest",
  "table-head",
  "field",
  "primary",
  "primary-foreground",
  "primary-container",
  "primary-container-foreground",
  "secondary",
  "secondary-foreground",
  "muted",
  "muted-foreground",
  "accent",
  "accent-foreground",
  "border",
  "input",
  "ring",
  "pattern-square",
  "pattern-diamond",
] as const;

export type AppColorToken = (typeof APP_COLOR_TOKENS)[number];

export type AppColorPresetId =
  | "blue"
  | "violet"
  | "teal"
  | "green"
  | "amber"
  | "rose";

/** A preset id, the Windows accent, neutral greys, or a `#rrggbb` colour. */
export type AppColor =
  | { kind: "preset"; id: AppColorPresetId }
  | { kind: "system" }
  | { kind: "mono" }
  | { kind: "custom"; hex: string };

export interface AppColorPreset {
  id: AppColorPresetId;
  /** The OKLCH hue, in degrees. Blue is the default and writes nothing. */
  hue: number;
  /** How strongly the hue is applied, as a factor of the blue theme's chroma. */
  chroma: number;
}

/** Six well-spaced hues: blue is today's look, the other five are its hue at the same tones. */
export const APP_COLOR_PRESETS: readonly AppColorPreset[] = [
  { id: "blue", hue: 258, chroma: 1 },
  { id: "violet", hue: 300, chroma: 0.95 },
  { id: "teal", hue: 190, chroma: 0.9 },
  { id: "green", hue: 150, chroma: 0.9 },
  { id: "amber", hue: 70, chroma: 1 },
  { id: "rose", hue: 20, chroma: 1 },
];

export const DEFAULT_APP_COLOR: AppColor = { kind: "preset", id: "blue" };

/** The blue values from `index.css`; the generator keeps their OKLCH lightness. */
const BASE: Record<AppColorToken, { light: string; dark: string }> = {
  brand: { light: "#4a7fd6", dark: "#4a7fd6" },
  background: { light: "#eff4ff", dark: "#031427" },
  foreground: { light: "#0b1c30", dark: "#d3e4fe" },
  "foreground-variant": { light: "#424752", dark: "#c2c6d4" },
  chrome: { light: "#f8f9ff", dark: "#000f21" },
  card: { light: "#ffffff", dark: "#0b1c30" },
  "card-foreground": { light: "#0b1c30", dark: "#d3e4fe" },
  popover: { light: "#ffffff", dark: "#102034" },
  "popover-foreground": { light: "#0b1c30", dark: "#d3e4fe" },
  "surface-container": { light: "#e5eeff", dark: "#102034" },
  "surface-high": { light: "#dce9ff", dark: "#1b2b3f" },
  "surface-highest": { light: "#d3e4fe", dark: "#26364a" },
  "table-head": { light: "#f3f7ff", dark: "#102034" },
  field: { light: "#ffffff", dark: "#000f21" },
  primary: { light: "#1f5db2", dark: "#abc7ff" },
  "primary-foreground": { light: "#ffffff", dark: "#002f66" },
  "primary-container": { light: "#d7e2ff", dark: "#5d90e8" },
  "primary-container-foreground": { light: "#001b3f", dark: "#00285a" },
  secondary: { light: "#dce9ff", dark: "#1b2b3f" },
  "secondary-foreground": { light: "#0b1c30", dark: "#d3e4fe" },
  muted: { light: "#dce9ff", dark: "#1b2b3f" },
  "muted-foreground": { light: "#4f5563", dark: "#a0a6b4" },
  accent: { light: "#e5eeff", dark: "#1b2b3f" },
  "accent-foreground": { light: "#0b1c30", dark: "#d3e4fe" },
  border: { light: "#d3dcec", dark: "#1d2d42" },
  input: { light: "#c2c6d4", dark: "#424752" },
  ring: { light: "#1f5db2", dark: "#abc7ff" },
  "pattern-square": { light: "#4a7fd6", dark: "#4a7fd6" },
  "pattern-diamond": { light: "#6f9fe6", dark: "#8fc0f7" },
};

/** The base values, exported so a test can check them against `index.css`. */
export const APP_COLOR_BASE = BASE;

/** The chroma of the blue brand colour; a custom colour is scaled against it. */
const BASE_CHROMA = 0.1446;

export interface Oklch {
  l: number;
  c: number;
  h: number;
}

function srgbToLinear(channel: number): number {
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(channel: number): number {
  return channel <= 0.0031308
    ? 12.92 * channel
    : 1.055 * channel ** (1 / 2.4) - 0.055;
}

/** OKLCH of a `#rrggbb` colour. */
export function hexToOklch(hex: string): Oklch {
  const value = Number.parseInt(hex.slice(1), 16);
  const r = srgbToLinear((value >> 16) / 255);
  const g = srgbToLinear(((value >> 8) & 0xff) / 255);
  const b = srgbToLinear((value & 0xff) / 255);
  const l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b;
  const m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b;
  const s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b;
  const lRoot = Math.cbrt(l);
  const mRoot = Math.cbrt(m);
  const sRoot = Math.cbrt(s);
  const L =
    0.2104542553 * lRoot + 0.793617785 * mRoot - 0.0040720468 * sRoot;
  const a =
    1.9779984951 * lRoot - 2.428592205 * mRoot + 0.4505937099 * sRoot;
  const bb =
    0.0259040371 * lRoot + 0.7827717662 * mRoot - 0.808675766 * sRoot;
  let h = (Math.atan2(bb, a) * 180) / Math.PI;
  if (h < 0) h += 360;
  return { l: L, c: Math.hypot(a, bb), h };
}

/** Linear sRGB for an OKLCH colour (may be outside 0..1 before gamut mapping). */
function oklchToLinear(l: number, c: number, h: number): [number, number, number] {
  const radians = (h * Math.PI) / 180;
  const a = c * Math.cos(radians);
  const b = c * Math.sin(radians);
  const lRoot = l + 0.3963377774 * a + 0.2158037573 * b;
  const mRoot = l - 0.1055613458 * a - 0.0638541728 * b;
  const sRoot = l - 0.0894841775 * a - 1.291485548 * b;
  const l3 = lRoot ** 3;
  const m3 = mRoot ** 3;
  const s3 = sRoot ** 3;
  return [
    4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
    -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
    -0.0041960863 * l3 - 0.7034186147 * m3 + 1.707614701 * s3,
  ];
}

function inGamut(l: number, c: number, h: number): boolean {
  const epsilon = 0.0001;
  return oklchToLinear(l, c, h).every(
    (channel) => channel >= -epsilon && channel <= 1 + epsilon,
  );
}

/** OKLCH to `#rrggbb`, lowering the chroma only as far as the sRGB gamut needs. */
export function oklchToHex(l: number, c: number, h: number): string {
  let chroma = Math.max(0, c);
  for (let step = 0; step < 32 && !inGamut(l, chroma, h); step += 1) {
    chroma *= 0.92;
  }
  const channels = oklchToLinear(l, chroma, h)
    .map((channel) => {
      const srgb = linearToSrgb(Math.min(1, Math.max(0, channel)));
      return Math.round(srgb * 255)
        .toString(16)
        .padStart(2, "0");
    })
    .join("");
  return `#${channels}`;
}

/** The text tone that reads on `hex`, keeping the app's own dark tone when it passes 4.5:1. */
function foregroundOn(hex: string, neutral = false): string {
  const readable = 4.5;
  if (neutral) {
    return contrastRatio(hex, "#ffffff") >= contrastRatio(hex, "#000000")
      ? "#ffffff"
      : "#000000";
  }
  if (contrastRatio(hex, "#061428") >= readable) return "#061428";
  if (contrastRatio(hex, "#ffffff") >= readable) return "#ffffff";
  return contrastRatio(hex, "#000000") >= contrastRatio(hex, "#ffffff")
    ? "#000000"
    : "#ffffff";
}

export interface AppColorPalette {
  light: Record<AppColorToken, string>;
  dark: Record<AppColorToken, string>;
}

interface HueSource {
  hue: number;
  chroma: number;
}

function isPresetId(value: unknown): value is AppColorPresetId {
  return APP_COLOR_PRESETS.some((preset) => preset.id === value);
}

function sourceFromColor(hex: string): HueSource {
  const { c, h } = hexToOklch(hex);
  if (c < 0.01) return { hue: 0, chroma: 0 };
  return { hue: h, chroma: Math.min(1.2, Math.max(0.15, c / BASE_CHROMA)) };
}

/**
 * The hue and chroma a choice applies. Blue is the default and needs no palette; a system accent
 * without a value also keeps the blue default until the backend answers.
 */
function sourceOf(choice: AppColor, accentHex?: string | null): HueSource | null {
  if (choice.kind === "preset") {
    const preset = APP_COLOR_PRESETS.find((item) => item.id === choice.id);
    if (!preset || preset.id === "blue") return null;
    return { hue: preset.hue, chroma: preset.chroma };
  }
  if (choice.kind === "mono") return { hue: 0, chroma: 0 };
  if (choice.kind === "custom") {
    return isHexColor(choice.hex) ? sourceFromColor(choice.hex) : null;
  }
  return accentHex && isHexColor(accentHex) ? sourceFromColor(accentHex) : null;
}

function paletteFor(source: HueSource): AppColorPalette {
  const build = (dark: boolean): Record<AppColorToken, string> => {
    const tones = {} as Record<AppColorToken, string>;
    for (const token of APP_COLOR_TOKENS) {
      const base = BASE[token][dark ? "dark" : "light"];
      const { l, c } = hexToOklch(base);
      tones[token] = oklchToHex(l, c * source.chroma, source.hue);
    }
    // Text on a filled action follows the generated colour, not the blue tone.
    const neutral = source.chroma === 0;
    tones["primary-foreground"] = foregroundOn(tones.primary, neutral);
    tones["primary-container-foreground"] = foregroundOn(
      tones["primary-container"],
      neutral,
    );
    return tones;
  };
  return { light: build(false), dark: build(true) };
}

/** The full palette of a choice, or null when it is the blue default (no override is written). */
export function appColorPalette(
  choice: AppColor,
  accentHex?: string | null,
): AppColorPalette | null {
  const source = sourceOf(choice, accentHex);
  return source ? paletteFor(source) : null;
}

/** The swatch colour of a choice in one theme. */
export function appColorSwatch(
  choice: AppColor,
  dark: boolean,
  accentHex?: string | null,
): string {
  if (choice.kind === "custom" && isHexColor(choice.hex)) return choice.hex;
  if (choice.kind === "system" && accentHex && isHexColor(accentHex)) {
    return accentHex;
  }
  const palette = appColorPalette(choice, accentHex);
  if (palette) return dark ? palette.dark.primary : palette.light.primary;
  return BASE.primary[dark ? "dark" : "light"];
}

/** The stylesheet that applies `choice` in both themes; the blue default writes nothing. */
export function appColorCss(
  choice: AppColor,
  accentHex?: string | null,
): string {
  const palette = appColorPalette(choice, accentHex);
  if (!palette) return "";
  const block = (selector: string, tones: Record<AppColorToken, string>): string =>
    `${selector} {\n  ${APP_COLOR_TOKENS.map(
      (token) => `--${token}: ${tones[token]};`,
    ).join("\n  ")}\n}`;
  return [block(":root", palette.light), block(".dark", palette.dark)].join(
    "\n",
  );
}

/** The normalised value of a stored or imported choice; null when it is not a valid one. */
export function normalizeAppColor(value: unknown): AppColor | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  const record = value as Record<string, unknown>;
  if (record.kind === "preset") {
    return isPresetId(record.id) ? { kind: "preset", id: record.id } : null;
  }
  if (record.kind === "system") return { kind: "system" };
  if (record.kind === "mono") return { kind: "mono" };
  if (record.kind === "custom") {
    return isHexColor(record.hex)
      ? { kind: "custom", hex: record.hex.toLowerCase() }
      : null;
  }
  return null;
}

/** A stored choice, with the blue default for anything unusable. */
export function parseAppColor(value: unknown): AppColor {
  return normalizeAppColor(value) ?? { ...DEFAULT_APP_COLOR };
}
