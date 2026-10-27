// @vitest-environment node
import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { baseName, dirName, engineLabel, exportFileName } from "./engine";
import { capitalize, describeError, errorParams } from "./errors";

const MESSAGES: Record<string, string> = {
  "errors.codes.SP-APP-002.reason": "no file is open",
  "errors.codes.SP-APP-002.fix": "Open a save file first.",
  "errors.codes.SP-APP-005.reason": "{{value}} is outside the allowed range for this field",
  "errors.codes.SP-APP-005.reasonPlain": "the number is outside the allowed range for this field",
  "errors.codes.SP-APP-005.fix": "Type a number inside the range shown for the field.",
  "errors.codes.SP-DET-001.reason": "SavePrism does not know this file yet",
  "errors.codes.SP-DET-001.fix": "Attach a sample file to an issue so support can be added.",
};

/** A tiny stand-in for i18next's `t`: key lookup, `defaultValue` and `{{name}}` interpolation. */
function fakeT(key: string, options?: Record<string, unknown>): string {
  const template =
    MESSAGES[key] ?? (typeof options?.defaultValue === "string" ? options.defaultValue : "");
  return template.replace(/\{\{(\w+)\}\}/g, (_, name: string) =>
    String(options?.[name] ?? ""),
  );
}
const t = fakeT as unknown as TFunction;

describe("describeError", () => {
  it("splits a host error into reason and fix", () => {
    const parts = describeError({
      code: "SP-APP-002",
      message: "no file is open",
      hint: "Open a save file first.",
    });
    expect(parts.reason).toBe("No file is open (SP-APP-002).");
    expect(parts.fix).toBe("Open a save file first.");
  });

  it("uses the translated text of a known code", () => {
    const parts = describeError(
      { code: "SP-APP-002", message: "no file is open", hint: "Open a save file first." },
      t,
    );
    expect(parts.reason).toBe("No file is open (SP-APP-002).");
    expect(parts.fix).toBe("Open a save file first.");
  });

  it("greets an unknown file in plain words", () => {
    const parts = describeError(
      {
        code: "SP-DET-001",
        message: "no layer or codec matched after 3 trial decodes",
        hint: "Attach a sample file to an issue so support can be added.",
      },
      t,
    );
    expect(parts.reason).toBe("SavePrism does not know this file yet (SP-DET-001).");
    expect(parts.fix).not.toMatch(/layer|codec|trial decode/i);
  });

  it("fills the parameters of a translated error from the host message", () => {
    const parts = describeError(
      {
        code: "SP-APP-005",
        message: "120 is outside the range of this value (0 to 99)",
        hint: "Type a number from 0 to 99.",
      },
      t,
    );
    expect(parts.reason).toBe(
      "120 is outside the allowed range for this field (SP-APP-005).",
    );
  });

  it("falls back to the host text for an unknown code", () => {
    const parts = describeError(
      { code: "SP-XXX-999", message: "something broke", hint: "Try again." },
      t,
    );
    expect(parts.reason).toBe("Something broke (SP-XXX-999).");
    expect(parts.fix).toBe("Try again.");
  });

  it("shows other values as plain text", () => {
    expect(describeError(new Error("boom")).reason).toBe("Boom.");
    expect(describeError("plain").fix).toBe("");
    expect(describeError(undefined).reason).toBe("Undefined.");
  });

  it("capitalizes only the first letter", () => {
    expect(capitalize("abc def")).toBe("Abc def");
    expect(capitalize("")).toBe("");
  });
});

describe("errorParams", () => {
  it("reads the value of a bad or duplicate value", () => {
    expect(errorParams("SP-APP-004", '"abc" is not a whole number')).toEqual({ value: "abc" });
    expect(errorParams("SP-APP-020", '"gold" is there already')).toEqual({ value: "gold" });
  });

  it("reads the value and range of an out-of-range number", () => {
    expect(errorParams("SP-APP-005", "120 is outside the range of this value (0 to 99)")).toEqual({
      value: "120",
      low: "0",
      high: "99",
    });
    expect(errorParams("SP-APP-005", "100 is outside the range of a count (1 to 99)")).toEqual({
      value: "100",
    });
  });

  it("reads the name of a missing backup", () => {
    expect(errorParams("SP-APP-013", 'there is no backup "x.bak" of this file')).toEqual({
      name: "x.bak",
    });
  });

  it("gives null for a code or message without parameters", () => {
    expect(errorParams("SP-APP-002", "no file is open")).toBeNull();
    expect(errorParams("SP-APP-005", "something else")).toBeNull();
  });
});

describe("engine helpers", () => {
  it("names the engine from the extension first, then the codec", () => {
    expect(engineLabel("file1.rmmzsave", "format.rpgmaker-json")).toBe("RPG Maker MZ");
    expect(engineLabel("file1.rpgsave", "format.rpgmaker-json")).toBe("RPG Maker MV");
    expect(engineLabel("slot.sav", "format.gvas")).toBe("Unreal");
    expect(engineLabel("Save1.rxdata", "format.ruby-marshal")).toBe("RPG Maker XP");
    expect(engineLabel("Save1.rvdata", "format.ruby-marshal")).toBe("RPG Maker VX");
    expect(engineLabel("Save01.rvdata2", "format.ruby-marshal")).toBe("RPG Maker VX Ace");
    expect(engineLabel("slot.rsv", "format.nrbf")).toBe("RAGS");
    expect(engineLabel("player.dat", "format.nrbf")).toBe(".NET BinaryFormatter");
    expect(engineLabel("data.bin", "format.ruby-marshal")).toBe("Ruby Marshal");
    expect(engineLabel("data.bin", "format.java-serialization")).toBe("Java");
    expect(engineLabel("player.save", "format.godot-variant")).toBe("Godot");
    expect(engineLabel("settings.cfg", "format.godot-config")).toBe("Godot");
    expect(engineLabel("player.tres", "format.godot-resource-text")).toBe("Godot");
    expect(engineLabel("player.res", "format.godot-resource")).toBe("Godot");
    expect(engineLabel("data.bin", "format.json")).toBe("JSON");
    expect(engineLabel("save.zip", "format.json")).toBe("JSON");
    expect(engineLabel("save.zip", "format.ruby-marshal")).toBe("Ruby Marshal");
    expect(engineLabel("shared.sol", "format.flash-sol")).toBe("Flash");
    expect(engineLabel("shared.bin", "format.flash-sol")).toBe("Flash LSO");
    expect(engineLabel("save.srm", "format.pokemon-gen3")).toBe("Pokémon Gen 3");
    expect(engineLabel("Save01.lsd", "format.lcf")).toBe("RPG Maker 2000/2003");
    expect(engineLabel("RPG_RT.ldb", "format.lcf")).toBe("RPG Maker 2000/2003");
    expect(engineLabel("prefs.reg", "format.reg")).toBe("Registry (.reg)");
    expect(engineLabel("data.bin", "format.reg")).toBe("Registry (.reg)");
    expect(engineLabel("game.sqlite", "format.sqlite")).toBe("SQLite");
    expect(engineLabel("game.sqlite3", "format.sqlite")).toBe("SQLite");
    expect(engineLabel("save.db", "format.sqlite")).toBe("SQLite");
    expect(engineLabel("game.db", "format.litedb")).toBe("LiteDB");
    expect(engineLabel("agssave.000", "format.ags")).toBe("Adventure Game Studio");
    expect(engineLabel("snapshot.000", "format.snes9x-snapshot")).toBe("Snes9x snapshot");
    expect(engineLabel("data.bin", "format.sqlite")).toBe("SQLite");
    expect(engineLabel("SaveData01.sav", "format.wolf-rpg")).toBe("Wolf RPG");
    expect(engineLabel("data.bin", "format.wolf-rpg")).toBe("Wolf RPG");
    expect(engineLabel("slot.sav", "format.qsp")).toBe("QSP");
    expect(engineLabel("chapter-one.sav", "format.qsp")).toBe("QSP");
    expect(engineLabel("ManualSave_1.sav", "format.witcher3")).toBe("The Witcher 3");
    expect(engineLabel("character.bic", "format.gff")).toBe("BioWare GFF");
    expect(engineLabel("SAVEGAME.sav", "format.gff")).toBe("BioWare GFF");
    expect(engineLabel("save.das", "format.gff4")).toBe("Dragon Age");
    expect(engineLabel("data.bin", "format.gff4")).toBe("Dragon Age");
    expect(engineLabel("module.mod", "container.erf")).toBe("Neverwinter Nights");
    expect(engineLabel("save.erf", "container.erf")).toBe("BioWare ERF");
    expect(engineLabel("data.bin", "container.erf")).toBe("BioWare ERF");
    expect(engineLabel("data.bin", "format.unknown")).toBeNull();
    expect(engineLabel("slot1.save", "format.json", "sugarcube")).toBe("SugarCube");
    expect(engineLabel("1-1-LT1.save", "format.python-pickle", "renpy")).toBe("Ren'Py");
    expect(engineLabel("persistent", "format.python-pickle", "renpy")).toBe("Ren'Py");
    expect(engineLabel("data.bin", "format.python-pickle")).toBe("Python pickle");
    expect(engineLabel("GameSave001.nson", "format.json", "naninovel")).toBe("Naninovel");
    expect(engineLabel("save.bin", "format.naninovel")).toBe("Naninovel");
    expect(engineLabel("slot.sav", "format.json", "tyranoscript")).toBe("TyranoScript");
    expect(engineLabel("file1.rpgsave", "format.json", null)).toBe("RPG Maker MV");
  });

  it("splits paths with either separator", () => {
    expect(baseName("C:\\saves\\a.sav")).toBe("a.sav");
    expect(baseName("/home/u/a.sav")).toBe("a.sav");
    expect(dirName("C:\\saves\\a.sav")).toBe("C:\\saves");
    expect(dirName("a.sav")).toBe("");
  });

  it("names the .txt export after the save's stem", () => {
    expect(exportFileName("file1.rmmzsave")).toBe("file1-export.json");
    expect(exportFileName("slot.v2.es3")).toBe("slot.v2-export.json");
    expect(exportFileName("SaveGame")).toBe("SaveGame-export.json");
    expect(exportFileName(".hidden")).toBe(".hidden-export.json");
    expect(exportFileName("compact.json")).toBe("compact-export.json");
  });
});
