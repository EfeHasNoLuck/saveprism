// @vitest-environment node
import { describe, expect, it } from "vitest";
import en from "../../locales/en/editor.json";
import tr from "../../locales/tr/editor.json";
import { askAiValueCount, buildAskAiText, MAX_ASK_AI_VALUES, type AskAiRow } from "./askAi";
import type { Translate } from "./helpers";

function flatten(obj: Record<string, unknown>, prefix = ""): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(obj)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object") {
      Object.assign(out, flatten(value as Record<string, unknown>, full));
    } else {
      out[full] = String(value);
    }
  }
  return out;
}

/** A `t` over a real locale bundle: looks the key up and fills its placeholders. */
function translate(bundle: Record<string, unknown>): Translate {
  const flat = flatten(bundle);
  return (key, options = {}) => {
    const template = flat[key] ?? key;
    return template.replace(/{{(\w+)}}/g, (_, name: string) =>
      String(options[name] ?? `{{${name}}}`),
    );
  };
}

const tEn = translate(en as Record<string, unknown>);
const tTr = translate(tr as Record<string, unknown>);

const rows: AskAiRow[] = [
  { name: "Potion", id: 1, path: "/party/_items/1", value: "12", kind: "int", min: 0, max: 99 },
  { name: null, id: 7, path: "/party/_items/7", value: "0", kind: "int", min: 0, max: 99 },
  { name: "Gold", path: "/party/_gold", value: "500", kind: "int", min: null, max: null },
];

describe("buildAskAiText", () => {
  it("writes the English note", () => {
    expect(
      buildAskAiText({
        t: tEn,
        fileName: "Save1.rpgsave",
        format: "RPG Maker MV",
        list: "Items",
        query: "potion",
        rows,
        total: 3,
      }),
    ).toBe(
      [
        "I am editing a game save with SavePrism.",
        "File: Save1.rpgsave (RPG Maker MV)",
        'List: Items, search "potion"',
        "Values (3):",
        "- Potion #1 = 12 (Whole number, 0 to 99) [/party/_items/1]",
        "- #7 = 0 (Whole number, 0 to 99) [/party/_items/7]",
        "- Gold = 500 [/party/_gold]",
        "Tell me which values to change and to what, by name. Keep each value inside its limits.",
      ].join("\n"),
    );
  });

  it("writes the Turkish note with the same values and paths", () => {
    expect(
      buildAskAiText({
        t: tTr,
        fileName: "Save1.rpgsave",
        format: "RPG Maker MV",
        list: "Eşyalar",
        query: "potion",
        rows,
        total: 3,
      }),
    ).toBe(
      [
        "SavePrism ile bir oyun kaydını düzenliyorum.",
        "Dosya: Save1.rpgsave (RPG Maker MV)",
        'Liste: Eşyalar, arama "potion"',
        "Değerler (3):",
        "- Potion #1 = 12 (Tam sayı, 0 ile 99 arası) [/party/_items/1]",
        "- #7 = 0 (Tam sayı, 0 ile 99 arası) [/party/_items/7]",
        "- Gold = 500 [/party/_gold]",
        "Hangi değerleri neye değiştireceğimi isimle söyle. Her değeri sınırları içinde tut.",
      ].join("\n"),
    );
  });

  it("leaves out the search and the format when there are none", () => {
    const text = buildAskAiText({
      t: tEn,
      fileName: "custom.dat",
      format: null,
      list: "Values",
      query: "  ",
      rows: [
        { name: "Gold", path: "/gold", value: "500", kind: "int", min: null, max: null },
      ],
      total: 1,
    });
    expect(text).toContain("File: custom.dat\n");
    expect(text).toContain("List: Values\n");
    expect(text).not.toContain("search");
  });

  it("shows on/off and text with their type", () => {
    const text = buildAskAiText({
      t: tEn,
      fileName: "Save1.rpgsave",
      format: "RPG Maker MV",
      list: "Switches",
      query: "",
      rows: [
        { name: "Switch", id: 1, path: "/switches/1", value: "true", kind: "bool", min: null, max: null },
        { name: "Motto", path: "/motto", value: "Hello", kind: "string", min: null, max: null },
      ],
      total: 2,
    });
    expect(text).toContain("- Switch #1 = true (True/false) [/switches/1]");
    expect(text).toContain("- Motto = Hello (Text) [/motto]");
  });

  it("leaves out the limits of a plain JSON number", () => {
    const edge = Number.MAX_SAFE_INTEGER;
    const text = buildAskAiText({
      t: tEn,
      fileName: "save.json",
      format: null,
      list: "Values",
      query: "",
      rows: [{ name: "level", path: "/level", value: "3", kind: "int", min: -edge, max: edge }],
      total: 1,
    });
    expect(text).toContain("- level = 3 [/level]");
  });

  it("copies only the first 300 values and says how many there are", () => {
    const many: AskAiRow[] = Array.from({ length: 305 }, (_, i) => ({
      name: null,
      id: i + 1,
      path: `/items/${i + 1}`,
      value: "1",
      kind: "int",
      min: null,
      max: null,
    }));
    const text = buildAskAiText({
      t: tEn,
      fileName: "Save1.rpgsave",
      format: "RPG Maker MV",
      list: "Items",
      query: "",
      rows: many,
      total: 1234,
    });
    expect(text).toContain(`Values (first ${MAX_ASK_AI_VALUES} of 1,234):`);
    expect(text.match(/^- /gm)).toHaveLength(MAX_ASK_AI_VALUES);
    expect(askAiValueCount(many)).toBe(MAX_ASK_AI_VALUES);
  });
});
