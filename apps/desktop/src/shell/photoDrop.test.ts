// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  cssPoint,
  isOverPhotoDrop,
  PHOTO_DROP_ATTRIBUTE,
  routeDrop,
  type DropHit,
} from "./photoDrop";

function hit(inside: boolean): DropHit {
  return {
    closest(selector: string) {
      expect(selector).toBe(`[${PHOTO_DROP_ATTRIBUTE}]`);
      return inside ? ({} as unknown) : null;
    },
  };
}

describe("routeDrop", () => {
  it("ignores an event without a path", () => {
    expect(routeDrop({ path: null, settingsOpen: false, overPhotoZone: false })).toBe("ignore");
    expect(routeDrop({ path: "", settingsOpen: true, overPhotoZone: true })).toBe("ignore");
  });

  it("opens a save while Settings is closed, wherever the drop lands", () => {
    expect(routeDrop({ path: "a.sav", settingsOpen: false, overPhotoZone: false })).toBe(
      "open-save",
    );
    expect(routeDrop({ path: "a.sav", settingsOpen: false, overPhotoZone: true })).toBe(
      "open-save",
    );
  });

  it("uses the photo over the zone while Settings is open", () => {
    expect(routeDrop({ path: "a.png", settingsOpen: true, overPhotoZone: true })).toBe("photo");
  });

  it("ignores a drop outside the zone while Settings is open, never opens a save", () => {
    expect(routeDrop({ path: "a.sav", settingsOpen: true, overPhotoZone: false })).toBe("ignore");
  });
});

describe("cssPoint", () => {
  it("converts physical pixels to CSS pixels with the display ratio", () => {
    expect(cssPoint({ x: 400, y: 300 }, 2)).toEqual({ x: 200, y: 150 });
    expect(cssPoint({ x: 150, y: 90 }, 1.5)).toEqual({ x: 100, y: 60 });
    expect(cssPoint({ x: 42, y: 7 }, 1)).toEqual({ x: 42, y: 7 });
  });

  it("falls back to 1 for a missing or impossible ratio", () => {
    expect(cssPoint({ x: 42, y: 7 }, 0)).toEqual({ x: 42, y: 7 });
    expect(cssPoint({ x: 42, y: 7 }, -2)).toEqual({ x: 42, y: 7 });
    expect(cssPoint({ x: 42, y: 7 }, Number.NaN)).toEqual({ x: 42, y: 7 });
  });
});

describe("isOverPhotoDrop", () => {
  it("hit-tests at the converted CSS position", () => {
    const asked: { x: number; y: number }[] = [];
    const over = isOverPhotoDrop({ x: 400, y: 300 }, 2, (x, y) => {
      asked.push({ x, y });
      return hit(true);
    });
    expect(over).toBe(true);
    expect(asked).toEqual([{ x: 200, y: 150 }]);
  });

  it("is false when the element is outside the zone or there is none", () => {
    expect(isOverPhotoDrop({ x: 10, y: 10 }, 1, () => hit(false))).toBe(false);
    expect(isOverPhotoDrop({ x: 10, y: 10 }, 1, () => null)).toBe(false);
  });
});
