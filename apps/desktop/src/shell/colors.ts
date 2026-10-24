/**
 * Pure colour settings: the presets of the three themed value groups and the CSS that applies
 * them. No DOM here, so the contrast test can run in a node environment.
 */

export const COLOR_GROUPS = ["changed", "accent", "error"] as const;

export type ColorGroup = (typeof COLOR_GROUPS)[number];

/** The preset id chosen for each group. */
export type ColorChoices = Record<ColorGroup, string>;

/** One theme's colours of a preset: text on the card, a filled container and the text on it. */
export interface PresetTones {
  main: string;
  container: string;
  onContainer: string;
}

export interface ColorPreset {
  id: string;
  light: PresetTones;
  dark: PresetTones;
}

const t = (main: string, container: string, onContainer: string): PresetTones => ({
  main,
  container,
  onContainer,
});

/**
 * Four presets per group. The first one of each group is the app's original colour from
 * `index.css`; the others are tonal sets of one hue at the same tones (light: 40 / 90 / 10;
 * dark: 80 / 30 / 90, the accent container 58 / 8), so they read like the originals.
 */
export const COLOR_PRESETS: Record<ColorGroup, readonly ColorPreset[]> = {
  changed: [
    { id: "violet", light: t("#6b38d4", "#e9ddff", "#23005c"), dark: t("#d0bcff", "#571bc1", "#e9ddff") },
    { id: "rose", light: t("#b01a75", "#ffd8e9", "#3d0025"), dark: t("#ffafd5", "#8b015a", "#ffd8e9") },
    { id: "indigo", light: t("#4352c3", "#e3dfff", "#00164f"), dark: t("#c6c0fe", "#1c3ca8", "#e3dfff") },
    { id: "amber", light: t("#8e4e04", "#fedcc2", "#2a1700"), dark: t("#ffb87a", "#6c3a01", "#fedcc2") },
  ],
  accent: [
    { id: "cyan", light: t("#00687a", "#acedff", "#001f26"), dark: t("#4cd7f6", "#009eb9", "#002f38") },
    { id: "blue", light: t("#1e5db2", "#dbe1fe", "#011b3d"), dark: t("#b5c4ff", "#5e89e4", "#011736") },
    { id: "green", light: t("#146c45", "#a1f4c5", "#002111"), dark: t("#86d7a9", "#4a9b70", "#011c0e") },
    { id: "amber", light: t("#875208", "#ffdcbd", "#291800"), dark: t("#fcb970", "#bb7f38", "#221501") },
  ],
  error: [
    { id: "red", light: t("#ba1a1a", "#ffdad6", "#410002"), dark: t("#ffb4ab", "#93000a", "#ffdad6") },
    { id: "orange", light: t("#aa3606", "#fedbce", "#311300"), dark: t("#feb599", "#832600", "#fedbce") },
    { id: "pink", light: t("#b51a59", "#ffd9e1", "#3f0019"), dark: t("#ffb1c3", "#8f0042", "#ffd9e1") },
    { id: "brown", light: t("#984725", "#ffdbcd", "#311300"), dark: t("#feb698", "#7b2f0f", "#ffdbcd") },
  ],
};

export const DEFAULT_COLORS: ColorChoices = {
  changed: "violet",
  accent: "cyan",
  error: "red",
};

/** True when `id` names a preset of `group`. */
export function isColorPreset(group: ColorGroup, id: unknown): id is string {
  return (
    typeof id === "string" &&
    COLOR_PRESETS[group].some((preset) => preset.id === id)
  );
}

/** The preset of `group` with `id`; an unknown id falls back to the group's first preset. */
export function presetFor(group: ColorGroup, id: string): ColorPreset {
  return COLOR_PRESETS[group].find((preset) => preset.id === id) ?? COLOR_PRESETS[group][0];
}

/**
 * The declarations a preset writes over the matching CSS variables of one theme, built the way
 * `index.css` builds the originals (dark rows are tinted with the container, light rows with the
 * text colour).
 */
function declarations(group: ColorGroup, preset: ColorPreset, dark: boolean): string[] {
  const { main, container, onContainer } = dark ? preset.dark : preset.light;
  if (group === "changed") {
    return [
      `--changed: ${main};`,
      `--changed-foreground: ${main};`,
      `--changed-row: color-mix(in srgb, ${dark ? container : main} ${dark ? 20 : 9}%, transparent);`,
      `--changed-chip: color-mix(in srgb, ${main} ${dark ? 20 : 14}%, transparent);`,
      `--changed-tag: ${container};`,
      `--changed-tag-foreground: ${onContainer};`,
    ];
  }
  if (group === "accent") {
    return [
      `--tertiary: ${main};`,
      `--tertiary-container: ${container};`,
      `--tertiary-container-foreground: ${onContainer};`,
    ];
  }
  return [
    `--destructive: ${main};`,
    `--error-row: color-mix(in srgb, ${dark ? container : main} ${dark ? 30 : 8}%, transparent);`,
    `--error-container: ${container};`,
    `--error-container-foreground: ${onContainer};`,
  ];
}

/**
 * The stylesheet that applies `choices` in both themes. A group left at its default colour writes
 * nothing, so the original `index.css` values stay exactly as they are.
 */
export function colorCss(choices: ColorChoices): string {
  const blocks: string[] = [];
  for (const dark of [false, true]) {
    const lines = COLOR_GROUPS.flatMap((group) => {
      if (choices[group] === DEFAULT_COLORS[group]) return [];
      return declarations(group, presetFor(group, choices[group]), dark);
    });
    if (lines.length > 0) {
      blocks.push(`${dark ? ".dark" : ":root"} {\n  ${lines.join("\n  ")}\n}`);
    }
  }
  return blocks.join("\n");
}
