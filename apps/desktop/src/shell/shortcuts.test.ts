// @vitest-environment node
import { describe, expect, it } from "vitest";
import { shortcutFor, type KeyInput } from "./shortcuts";

function press(key: string, extra: Partial<KeyInput> = {}): KeyInput {
  return { key, ctrlKey: true, metaKey: false, shiftKey: false, altKey: false, ...extra };
}

describe("shortcutFor", () => {
  it("maps Ctrl plus a letter to its action", () => {
    expect(shortcutFor(press("o"))).toBe("open");
    expect(shortcutFor(press("s"))).toBe("save");
    expect(shortcutFor(press("z"))).toBe("undo");
    expect(shortcutFor(press("y"))).toBe("redo");
    expect(shortcutFor(press("f"))).toBe("find");
  });

  it("uses Shift for Save as and Redo", () => {
    expect(shortcutFor(press("S", { shiftKey: true }))).toBe("saveAs");
    expect(shortcutFor(press("Z", { shiftKey: true }))).toBe("redo");
    expect(shortcutFor(press("O", { shiftKey: true }))).toBeNull();
    expect(shortcutFor(press("Y", { shiftKey: true }))).toBeNull();
    expect(shortcutFor(press("F", { shiftKey: true }))).toBeNull();
  });

  it("ignores a plain letter, other letters and Alt (AltGr) combinations", () => {
    expect(shortcutFor(press("z", { ctrlKey: false }))).toBeNull();
    expect(shortcutFor(press("q"))).toBeNull();
    expect(shortcutFor(press("Enter"))).toBeNull();
    expect(shortcutFor(press("s", { altKey: true }))).toBeNull();
  });

  it("accepts Cmd and a letter key on a layout without Latin letters", () => {
    expect(shortcutFor(press("z", { ctrlKey: false, metaKey: true }))).toBe("undo");
    expect(shortcutFor(press("я", { code: "KeyZ" }))).toBe("undo");
    expect(shortcutFor(press("ы", { code: "KeyS", shiftKey: true }))).toBe("saveAs");
    expect(shortcutFor(press("я", { code: "Digit1" }))).toBeNull();
  });
});
