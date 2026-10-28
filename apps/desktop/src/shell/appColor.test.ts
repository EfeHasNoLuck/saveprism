// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  APP_COLOR_BASE,
  APP_COLOR_PRESETS,
  APP_COLOR_TOKENS,
  appColorCss,
  appColorPalette,
  appColorSwatch,
  DEFAULT_APP_COLOR,
  hexToOklch,
  normalizeAppColor,
  parseAppColor,
  type AppColor,
} from "./appColor";
import { contrastRatio } from "./colors";

const HEX = /^#[0-9a-f]{6}$/;

const NON_BLUE_PRESETS: AppColor[] = APP_COLOR_PRESETS.filter(
  (preset) => preset.id !== "blue",
).map((preset) => ({ kind: "preset", id: preset.id }));

const EXTREME_CUSTOM = [
  "#000000",
  "#ffffff",
  "#ff0000",
  "#00ff00",
  "#0000ff",
  "#ffff00",
  "#00ffff",
  "#ff00ff",
  "#808080",
  "#123456",
];

describe("app colour palettes", () => {
  it("writes nothing for the blue default", () => {
    expect(appColorCss(DEFAULT_APP_COLOR)).toBe("");
    expect(appColorPalette(DEFAULT_APP_COLOR)).toBeNull();
  });

  it("has every surface and action token for dark and light", () => {
    const choices: AppColor[] = [
      ...NON_BLUE_PRESETS,
      { kind: "mono" },
      { kind: "custom", hex: "#8844cc" },
      { kind: "system" },
    ];
    for (const choice of choices) {
      const palette = appColorPalette(
        choice,
        choice.kind === "system" ? "#d67f4a" : undefined,
      );
      expect(palette).not.toBeNull();
      if (!palette) continue;
      for (const theme of ["light", "dark"] as const) {
        expect(Object.keys(palette[theme]).sort()).toEqual(
          [...APP_COLOR_TOKENS].sort(),
        );
        for (const token of APP_COLOR_TOKENS) {
          expect(palette[theme][token]).toMatch(HEX);
        }
      }
    }
  });

  it("writes both theme blocks for a non-blue choice", () => {
    const css = appColorCss({ kind: "preset", id: "violet" });
    expect(css).toContain(":root {");
    expect(css).toContain(".dark {");
    expect(css).toContain("--primary:");
    expect(css).toContain("--background:");
    expect(css).toContain("--pattern-diamond:");
  });

  it("keeps text on the primary and its container at 4.5:1 or more", () => {
    const choices: AppColor[] = [
      ...NON_BLUE_PRESETS,
      ...EXTREME_CUSTOM.map((hex): AppColor => ({ kind: "custom", hex })),
      { kind: "system" },
    ];
    for (const choice of choices) {
      const palette = appColorPalette(
        choice,
        choice.kind === "system" ? "#ff00aa" : undefined,
      );
      expect(palette).not.toBeNull();
      if (!palette) continue;
      for (const theme of ["light", "dark"] as const) {
        const tones = palette[theme];
        expect(
          contrastRatio(tones.primary, tones["primary-foreground"]),
          `primary on ${JSON.stringify(choice)} ${theme}`,
        ).toBeGreaterThanOrEqual(4.5);
        expect(
          contrastRatio(
            tones["primary-container"],
            tones["primary-container-foreground"],
          ),
          `container on ${JSON.stringify(choice)} ${theme}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("has no hue in monochrome", () => {
    const palette = appColorPalette({ kind: "mono" });
    expect(palette).not.toBeNull();
    if (!palette) return;
    for (const theme of ["light", "dark"] as const) {
      for (const token of APP_COLOR_TOKENS) {
        expect(hexToOklch(palette[theme][token]).c).toBeLessThan(0.001);
      }
    }
  });

  it("keeps the blue base values that the generator copies", () => {
    // The tokens whose lightness the palette reuses; the default leaves them untouched.
    expect(APP_COLOR_BASE.background).toEqual({
      light: "#eff4ff",
      dark: "#031427",
    });
    expect(APP_COLOR_BASE.primary).toEqual({
      light: "#1f5db2",
      dark: "#abc7ff",
    });
  });

  it("takes its hue from the Windows accent or a custom colour", () => {
    const red = appColorPalette({ kind: "system" }, "#ff0000");
    const green = appColorPalette({ kind: "system" }, "#00ff00");
    expect(red).not.toBeNull();
    expect(green).not.toBeNull();
    if (!red || !green) return;
    expect(red.light.primary).not.toBe(green.light.primary);
    const custom = appColorPalette({ kind: "custom", hex: "#00ff00" });
    expect(custom?.light.primary).toBe(green.light.primary);
  });

  it("gives a system choice with no accent the blue default", () => {
    expect(appColorCss({ kind: "system" })).toBe("");
  });

  it("shows a swatch for every choice", () => {
    expect(appColorSwatch(DEFAULT_APP_COLOR, false)).toBe("#1f5db2");
    expect(appColorSwatch(DEFAULT_APP_COLOR, true)).toBe("#abc7ff");
    expect(appColorSwatch({ kind: "custom", hex: "#123456" }, false)).toBe(
      "#123456",
    );
    expect(appColorSwatch({ kind: "system" }, false, "#abcdef")).toBe("#abcdef");
    expect(appColorSwatch({ kind: "preset", id: "violet" }, false)).toMatch(HEX);
  });
});

describe("app colour values", () => {
  it("accepts every documented form", () => {
    const values = [
      { kind: "preset", id: "blue" },
      { kind: "preset", id: "rose" },
      { kind: "system" },
      { kind: "mono" },
      { kind: "custom", hex: "#AABBCC" },
    ];
    for (const value of values) {
      expect(normalizeAppColor(value)).not.toBeNull();
    }
    expect(normalizeAppColor({ kind: "custom", hex: "#AABBCC" })).toEqual({
      kind: "custom",
      hex: "#aabbcc",
    });
  });

  it("refuses an unknown kind, preset or colour", () => {
    const values = [
      null,
      "blue",
      [],
      {},
      { kind: "other" },
      { kind: "preset", id: "pink" },
      { kind: "preset" },
      { kind: "custom", hex: "#12345" },
      { kind: "custom", hex: "red" },
    ];
    for (const value of values) {
      expect(normalizeAppColor(value)).toBeNull();
    }
  });

  it("falls back to blue for stored text that is not a choice", () => {
    expect(parseAppColor(undefined)).toEqual(DEFAULT_APP_COLOR);
    expect(parseAppColor("violet")).toEqual(DEFAULT_APP_COLOR);
    expect(parseAppColor({ kind: "custom", hex: "nope" })).toEqual(
      DEFAULT_APP_COLOR,
    );
  });
});
