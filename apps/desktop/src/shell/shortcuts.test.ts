// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  combinationFromInput,
  conflictingAction,
  defaultShortcuts,
  formatCombination,
  parseCombination,
  parseStoredShortcuts,
  removeShortcut,
  resetAllShortcuts,
  resetShortcut,
  shortcutFor,
  validateCombination,
  validateCombinationText,
  withShortcut,
  type KeyInput,
} from "./shortcuts";

function press(key: string, extra: Partial<KeyInput> = {}): KeyInput {
  return { key, ctrlKey: true, metaKey: false, shiftKey: false, altKey: false, ...extra };
}

describe("formatCombination / parseCombination", () => {
  it("formats the modifiers in one order whatever the input order", () => {
    const combination = parseCombination("shift+ctrl+s");
    expect(combination).not.toBeNull();
    expect(formatCombination(combination!)).toBe("Ctrl+Shift+S");
    expect(formatCombination(parseCombination("Alt+Ctrl+X")!)).toBe("Ctrl+Alt+X");
  });

  it("reads the key case-insensitively and keeps function and punctuation keys", () => {
    expect(formatCombination(parseCombination("ctrl+f12")!)).toBe("Ctrl+F12");
    expect(formatCombination(parseCombination("Ctrl+,")!)).toBe("Ctrl+,");
    expect(formatCombination(parseCombination("ctrl+space")!)).toBe("Ctrl+Space");
    expect(formatCombination(parseCombination("ctrl+delete")!)).toBe("Ctrl+Delete");
  });

  it("refuses text that is not one combination of modifiers and a key", () => {
    expect(parseCombination("Ctrl")).toBeNull();
    expect(parseCombination("Ctrl+S+O")).toBeNull();
    expect(parseCombination("Ctrl++")).toBeNull();
    expect(parseCombination("Ctrl+Wibble")).toBeNull();
    expect(parseCombination("")).toBeNull();
  });

  it("builds a combination from a key press, with the physical key on a non-Latin layout", () => {
    expect(formatCombination(combinationFromInput(press("s"))!)).toBe("Ctrl+S");
    expect(formatCombination(combinationFromInput(press("ы", { code: "KeyS" }))!)).toBe("Ctrl+S");
    expect(formatCombination(combinationFromInput(press("я", { code: "KeyZ" }))!)).toBe("Ctrl+Z");
    expect(formatCombination(combinationFromInput(press("1", { code: "Digit1" }))!)).toBe("Ctrl+1");
  });

  it("never makes a combination out of Enter or Esc", () => {
    expect(combinationFromInput(press("Enter"))).toBeNull();
    expect(combinationFromInput(press("Escape"))).toBeNull();
  });

  it("never records the Windows key", () => {
    expect(combinationFromInput(press("k", { ctrlKey: false, metaKey: true }))).toBeNull();
    expect(combinationFromInput(press("k", { metaKey: true }))).toBeNull();
  });
});

describe("validateCombination", () => {
  it("accepts a modifier and one key, including function keys on their own", () => {
    expect(validateCombination(parseCombination("Ctrl+S")!)).toBeNull();
    expect(validateCombination(parseCombination("Alt+S")!)).toBeNull();
    expect(validateCombination(parseCombination("F5")!)).toBeNull();
  });

  it("refuses more than two modifiers plus a key (3 keys at most)", () => {
    expect(validateCombinationText("Ctrl+Shift+Alt+S")).toBe("threeKeys");
  });

  it("needs Ctrl or Alt unless the key is F1-F12", () => {
    expect(validateCombinationText("S")).toBe("needCtrlOrAlt");
    expect(validateCombinationText("Shift+S")).toBe("needCtrlOrAlt");
    expect(validateCombinationText("Shift+F5")).toBeNull();
  });

  it("refuses Ctrl+Alt together because of AltGr", () => {
    expect(validateCombinationText("Ctrl+Alt+S")).toBe("ctrlAlt");
    expect(validateCombinationText("Ctrl+Alt+Del")).not.toBeNull();
  });

  it("refuses the system combinations", () => {
    expect(validateCombinationText("Alt+F4")).toBe("system");
    expect(validateCombinationText("Alt+Tab")).toBe("system");
    expect(validateCombinationText("Alt+Esc")).toBe("system");
    expect(validateCombinationText("Alt+Space")).toBe("system");
    expect(validateCombinationText("Ctrl+Esc")).toBe("system");
    expect(validateCombinationText("Ctrl+Shift+Esc")).toBe("system");
  });

  it("refuses Enter and Esc and text it cannot read", () => {
    expect(validateCombinationText("Ctrl+Enter")).toBe("reserved");
    expect(validateCombinationText("Shift+Escape")).toBe("reserved");
    expect(validateCombinationText("Ctrl+Wibble")).toBe("unknown");
  });
});

describe("shortcutFor", () => {
  it("maps the default bindings of the app", () => {
    expect(shortcutFor(press("o"))).toBe("open");
    expect(shortcutFor(press("s"))).toBe("save");
    expect(shortcutFor(press("S", { shiftKey: true }))).toBe("saveAs");
    expect(shortcutFor(press("z"))).toBe("undo");
    expect(shortcutFor(press("y"))).toBe("redo");
    expect(shortcutFor(press("Z", { shiftKey: true }))).toBe("redo");
    expect(shortcutFor(press("f"))).toBe("find");
  });

  it("ignores a plain letter, other letters and refused combinations", () => {
    expect(shortcutFor(press("z", { ctrlKey: false }))).toBeNull();
    expect(shortcutFor(press("q"))).toBeNull();
    expect(shortcutFor(press("Enter"))).toBeNull();
    expect(shortcutFor(press("s", { altKey: true }))).toBeNull();
  });

  it("ignores the Windows key (metaKey) and reads a non-Latin layout by its physical key", () => {
    expect(shortcutFor(press("z", { ctrlKey: false, metaKey: true }))).toBeNull();
    expect(shortcutFor(press("k", { ctrlKey: false, metaKey: true }))).toBeNull();
    expect(shortcutFor(press("я", { code: "KeyZ" }))).toBe("undo");
    expect(shortcutFor(press("ы", { code: "KeyS", shiftKey: true }))).toBe("saveAs");
    expect(shortcutFor(press("я", { code: "Digit1" }))).toBeNull();
  });

  it("follows the bindings the player chose", () => {
    const bindings = { ...defaultShortcuts(), save: ["Alt+S"], open: [] };
    expect(shortcutFor(press("s", { ctrlKey: false, altKey: true }), bindings)).toBe("save");
    expect(shortcutFor(press("o"), bindings)).toBeNull();
    expect(shortcutFor(press("s"), bindings)).toBeNull();
  });
});

describe("editing the bindings", () => {
  it("takes at most two shortcuts per action", () => {
    let bindings = withShortcut(defaultShortcuts(), "find", "Ctrl+G");
    expect(bindings.find).toEqual(["Ctrl+F", "Ctrl+G"]);
    bindings = withShortcut(bindings, "find", "Ctrl+H");
    expect(bindings.find).toEqual(["Ctrl+F", "Ctrl+G"]);
  });

  it("replaces the slot that was clicked", () => {
    let bindings = withShortcut(defaultShortcuts(), "find", "Ctrl+G");
    bindings = withShortcut(bindings, "find", "Alt+F", 0);
    expect(bindings.find).toEqual(["Alt+F", "Ctrl+G"]);
  });

  it("moves a conflicting shortcut away from the action that had it", () => {
    const before = defaultShortcuts();
    expect(conflictingAction(before, "find", "Ctrl+S")).toBe("save");
    const after = withShortcut(before, "find", "Ctrl+S");
    expect(after.find).toEqual(["Ctrl+F", "Ctrl+S"]);
    expect(after.save).toEqual([]);
    expect(conflictingAction(after, "find", "Ctrl+S")).toBeNull();
  });

  it("removes one shortcut and resets one action or all of them", () => {
    const custom = withShortcut(defaultShortcuts(), "save", "Alt+S", 0);
    expect(removeShortcut(custom, "save", "Alt+S").save).toEqual([]);
    expect(resetShortcut(custom, "save").save).toEqual(["Ctrl+S"]);
    expect(resetAllShortcuts()).toEqual(defaultShortcuts());
    expect(resetAllShortcuts()).not.toBe(defaultShortcuts());
  });
});

describe("parseStoredShortcuts", () => {
  it("gives the defaults for missing or unusable storage", () => {
    expect(parseStoredShortcuts(undefined)).toEqual(defaultShortcuts());
    expect(parseStoredShortcuts(null)).toEqual(defaultShortcuts());
    expect(parseStoredShortcuts([])).toEqual(defaultShortcuts());
    expect(parseStoredShortcuts("Ctrl+S")).toEqual(defaultShortcuts());
    expect(parseStoredShortcuts({ save: "Ctrl+S" }).save).toEqual(["Ctrl+S"]);
  });

  it("keeps an empty list as Not set", () => {
    expect(parseStoredShortcuts({ save: [] }).save).toEqual([]);
  });

  it("canonicalises good values and falls back per action on broken ones", () => {
    const parsed = parseStoredShortcuts({
      find: ["ctrl+shift+f"],
      open: ["Ctrl+Alt+O"],
      save: ["Alt+S", 42],
      redo: ["Ctrl+Y", "Ctrl+Shift+Z", "Alt+R"],
      undo: ["Ctrl+Z", "Ctrl+Z"],
    });
    expect(parsed.find).toEqual(["Ctrl+Shift+F"]);
    expect(parsed.open).toEqual(["Ctrl+O"]);
    expect(parsed.save).toEqual(["Ctrl+S"]);
    expect(parsed.redo).toEqual(["Ctrl+Y", "Ctrl+Shift+Z"]);
    expect(parsed.undo).toEqual(["Ctrl+Z"]);
  });

  it("leaves an action at its default when the stored object does not mention it", () => {
    expect(parseStoredShortcuts({ find: ["Alt+F"] }).save).toEqual(["Ctrl+S"]);
  });
});
