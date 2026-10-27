// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { Field, GameView, RpgMakerView, ValueRow } from "@/api/commands";
import en from "../../locales/en/editor.json";
import tr from "../../locales/tr/editor.json";
import type { Translate } from "./helpers";
import { entryLabel, pickLabels, viewLabels } from "./labels";

function translate(bundle: Record<string, unknown>): Translate {
  const look = (key: string): unknown =>
    key.split(".").reduce<unknown>((node, part) => (node as Record<string, unknown>)?.[part], bundle);
  return (key, options = {}) => {
    const found = look(key);
    const template = typeof found === "string" ? found : String(options.defaultValue ?? key);
    return template.replace(/{{(\w+)}}/g, (_, name: string) => String(options[name] ?? ""));
  };
}

const tEn = translate(en as Record<string, unknown>);
const tTr = translate(tr as Record<string, unknown>);

function field(path: string): Field {
  return {
    path,
    value: "1",
    original: "1",
    kind: "int",
    typeName: null,
    min: null,
    max: null,
    hint: "",
    editable: true,
    readOnlyReason: null,
    added: false,
  };
}

function valueRow(name: string, path: string): ValueRow {
  return {
    name,
    path,
    field: field(path),
    children: 0,
    preview: "",
    note: null,
    addable: false,
    removable: false,
  };
}

const rpg: RpgMakerView = {
  engine: "mv",
  gold: field("/party/_gold"),
  actors: [
    {
      id: 2,
      name: "Platina",
      level: field("/actors/_data/@a/2/_level"),
      levelUpdatesExp: true,
      hp: field("/actors/_data/@a/2/_hp"),
      mp: field("/actors/_data/@a/2/_mp"),
      exp: null,
      bonuses: [{ key: "attack", field: field("/actors/_data/@a/2/_paramPlus/@a/2") }],
    },
  ],
  items: [{ id: 1, name: "Potion", field: field("/party/_items/1"), removable: true }],
  weapons: [{ id: 4, name: null, field: field("/party/_weapons/4"), removable: true }],
  armors: [],
  variables: [],
  switches: [{ id: 1, name: "Limbo", field: field("/switches/_data/@a/1"), removable: false }],
  namesFolder: null,
  catalog: { items: [], weapons: [], armors: [] },
  canAdd: { items: true, weapons: true, armors: true },
};

describe("viewLabels", () => {
  it("names the RPG Maker sections in the UI language", () => {
    const labels = viewLabels({ kind: "rpgmaker", view: rpg }, tEn);
    expect(labels.get("/party/_gold")).toBe("Gold");
    expect(labels.get("/actors/_data/@a/2/_level")).toBe("Platina · Level");
    expect(labels.get("/actors/_data/@a/2/_paramPlus/@a/2")).toBe("Platina · Attack");
    expect(labels.get("/party/_items/1")).toBe("Item #1 Potion");
    expect(labels.get("/party/_weapons/4")).toBe("Weapon #4");
    expect(labels.get("/switches/_data/@a/1")).toBe("Switch #1 Limbo");
    const turkish = viewLabels({ kind: "rpgmaker", view: rpg }, tTr);
    expect(turkish.get("/switches/_data/@a/1")).toBe("Anahtar #1 Limbo");
  });

  it("names the rows of a Values list and its file details", () => {
    const view: GameView = {
      kind: "values",
      rows: [valueRow("playermoneyarray_0", "/root/properties/playermoneyarray_0/0")],
      addParent: null,
      details: [valueRow("save_game_type", "/root/save_game_type")],
    };
    const labels = viewLabels(view, tEn);
    expect(labels.get("/root/properties/playermoneyarray_0/0")).toBe("playermoneyarray_0");
    expect(labels.get("/root/save_game_type")).toBe("save_game_type");
  });
});

describe("entryLabel", () => {
  it("keeps the number in front of a blank name", () => {
    expect(entryLabel("variable", { id: 7, name: "  " }, tEn)).toBe("Variable #7");
  });
});

describe("pickLabels", () => {
  it("takes the view's name, else the host's, else the path", () => {
    const labels = pickLabels(
      ["/party/_gold", "/a/b", "/c"],
      new Map([["/party/_gold", "Gold"]]),
      ["party · _gold", "a · b", ""],
    );
    expect([...labels.values()]).toEqual(["Gold", "a · b", "/c"]);
  });
});
