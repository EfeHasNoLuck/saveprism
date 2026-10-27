// @vitest-environment node
import { describe, expect, it } from "vitest";
import en from "../../locales/en/editor.json";
import tr from "../../locales/tr/editor.json";
import {
  askAiValueCount,
  buildAskAiText,
  collectAskAiRows,
  MAX_ASK_AI_DEPTH,
  MAX_ASK_AI_VALUES,
  MAX_ASK_AI_VISITS,
  type AskAiChild,
  type AskAiRow,
  type AskAiSource,
} from "./askAi";
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
        "- Potion #1 = 12 (Whole number, 0 to 99)",
        "- #7 = 0 (Whole number, 0 to 99)",
        "- Gold = 500",
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
        "- Potion #1 = 12 (Tam sayı, 0 ile 99 arası)",
        "- #7 = 0 (Tam sayı, 0 ile 99 arası)",
        "- Gold = 500",
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
    expect(text).toContain("- Switch #1 = true (True/false)");
    expect(text).toContain("- Motto = Hello (Text)");
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
    expect(text).toContain("- level = 3");
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

  it("says the file has more when the total is not known", () => {
    const one: AskAiRow[] = [
      { name: "hp", path: "/player/0/hp", value: "10", kind: "int", min: null, max: null },
    ];
    const input = { fileName: "game.db", format: null, list: "Values", query: "", rows: one, total: null };
    expect(buildAskAiText({ ...input, t: tEn })).toContain("Values (first 1; the file has more):\n");
    expect(buildAskAiText({ ...input, t: tTr })).toContain(
      "Değerler (ilk 1; dosyada daha fazlası var):\n",
    );
  });
});

/**
 * A fake host over a nested object: objects and arrays are groups, numbers are whole-number values,
 * text is text. Counts the children it hands out.
 */
function fakeSource(tree: Record<string, unknown>, pageSize = 2) {
  const at = (path: string): unknown =>
    path
      .split("/")
      .slice(1)
      .reduce<unknown>((node, key) => (node as Record<string, unknown>)[key], tree);
  let listed = 0;
  const source: AskAiSource = {
    children: (path, offset) => {
      const node = at(path) as Record<string, unknown>;
      const all: AskAiChild[] = Object.entries(node).map(([key, value]) => ({
        key,
        path: `${path}/${key}`,
        kind:
          typeof value === "object" && value !== null
            ? Array.isArray(value)
              ? "array"
              : "object"
            : typeof value === "number"
              ? "int"
              : typeof value === "string"
                ? "string"
                : "null",
      }));
      const items = all.slice(offset, offset + pageSize);
      listed += items.length;
      return Promise.resolve({ items, total: all.length });
    },
    field: (path) => {
      const value = at(path);
      return Promise.resolve({
        path,
        value: String(value),
        kind: typeof value === "number" ? "int" : "string",
        min: null,
        max: null,
      });
    },
  };
  return { source, listed: () => listed };
}

describe("collectAskAiRows", () => {
  const GROUPS = [
    { path: "/player", name: "Player" },
    { path: "/world", name: "World" },
  ];
  const tree = {
    player: {
      name: "Aria",
      stats: { hp: 10, mp: 4 },
      gold: 1250,
      tags: [],
      nothing: null,
    },
    world: { day: 3 },
  };

  it("reads the values inside groups in the order the card shows them", async () => {
    const own: AskAiRow = { name: "version", path: "/version", value: "2", kind: "int", min: null, max: null };
    const { source } = fakeSource(tree);
    const result = await collectAskAiRows([own], GROUPS, "", source);
    expect(result.rows.map((row) => `${row.name}=${row.value} ${row.path}`)).toEqual([
      "version=2 /version",
      "Player · name=Aria /player/name",
      "Player · gold=1250 /player/gold",
      "Player · stats · hp=10 /player/stats/hp",
      "Player · stats · mp=4 /player/stats/mp",
      "World · day=3 /world/day",
    ]);
    expect(result.total).toBe(6);
  });

  it("keeps only the values whose name, a group's name on the way, or value holds the search", async () => {
    const { source } = fakeSource(tree);
    const byName = await collectAskAiRows([], GROUPS, " HP ", source);
    expect(byName.rows.map((row) => row.path)).toEqual(["/player/stats/hp"]);
    const byGroup = await collectAskAiRows([], GROUPS, "stats", source);
    expect(byGroup.rows.map((row) => row.path)).toEqual(["/player/stats/hp", "/player/stats/mp"]);
    const byValue = await collectAskAiRows([], GROUPS, "aria", source);
    expect(byValue.rows.map((row) => row.path)).toEqual(["/player/name"]);
    expect(byValue.total).toBe(1);
  });

  it("stops one value past the note's limit and leaves the total unknown", async () => {
    const big = { list: Object.fromEntries(Array.from({ length: 500 }, (_, i) => [`v${i}`, i])) };
    const { source, listed } = fakeSource(big, 200);
    const result = await collectAskAiRows([], [{ path: "/list", name: "list" }], "", source);
    expect(result.rows).toHaveLength(MAX_ASK_AI_VALUES + 1);
    expect(result.total).toBeNull();
    expect(listed()).toBeLessThanOrEqual(400);
    const text = buildAskAiText({
      t: tEn,
      fileName: "big.json",
      format: null,
      list: "Values",
      query: "",
      rows: result.rows,
      total: result.total,
    });
    expect(text).toContain(`Values (first ${MAX_ASK_AI_VALUES}; the file has more):`);
    expect(text.match(/^- /gm)).toHaveLength(MAX_ASK_AI_VALUES);
  });

  it("reads at most a bounded number of children and levels", async () => {
    // Many groups with no values: the walk gives up after MAX_ASK_AI_VISITS children.
    const wide = {
      groups: Object.fromEntries(Array.from({ length: MAX_ASK_AI_VISITS + 50 }, (_, i) => [`g${i}`, {}])),
    };
    const many = fakeSource(wide, 500);
    const result = await collectAskAiRows([], [{ path: "/groups", name: "groups" }], "", many.source);
    expect(result.total).toBeNull();
    expect(many.listed()).toBeLessThanOrEqual(MAX_ASK_AI_VISITS + 500);

    // A chain deeper than MAX_ASK_AI_DEPTH: the values below it are not read.
    let deep: Record<string, unknown> = { leaf: 1 };
    for (let i = 0; i < MAX_ASK_AI_DEPTH + 2; i += 1) deep = { next: deep };
    const chain = fakeSource({ root: deep });
    const below = await collectAskAiRows([], [{ path: "/root", name: "root" }], "", chain.source);
    expect(below.rows).toHaveLength(0);
    expect(below.total).toBeNull();
  });

  it("skips a group the host refuses", async () => {
    const { source } = fakeSource(tree);
    const refusing: AskAiSource = {
      ...source,
      children: (path, offset) => (path === "/player" ? Promise.resolve(null) : source.children(path, offset)),
    };
    const result = await collectAskAiRows([], GROUPS, "", refusing);
    expect(result.rows.map((row) => row.path)).toEqual(["/world/day"]);
  });

  it("takes a value the host listed with its child instead of asking again", async () => {
    let asked = 0;
    const source: AskAiSource = {
      children: () =>
        Promise.resolve({
          items: [
            {
              key: "Gold",
              path: "/p/0/value",
              kind: "int",
              field: { path: "/p/0/value", value: "7", kind: "int", min: null, max: null },
            },
          ],
          total: 1,
        }),
      field: () => {
        asked += 1;
        return Promise.resolve(null);
      },
    };
    const result = await collectAskAiRows([], [{ path: "/p", name: "Purse" }], "", source);
    expect(result.rows.map((row) => `${row.name}=${row.value}`)).toEqual(["Purse · Gold=7"]);
    expect(asked).toBe(0);
  });
});
