// @vitest-environment node
import { describe, expect, it } from "vitest";
import { readStored, whatsNewDecision, writeStored } from "./onboarding";

describe("whatsNewDecision", () => {
  it("does not open on the very first launch and stores the version", () => {
    expect(whatsNewDecision(null, "0.1.0")).toEqual({ show: false, store: "0.1.0" });
    expect(whatsNewDecision("", "0.1.0")).toEqual({ show: false, store: "0.1.0" });
  });

  it("does not open again for the same version", () => {
    expect(whatsNewDecision("0.1.0", "0.1.0")).toEqual({ show: false, store: null });
  });

  it("opens after an update", () => {
    expect(whatsNewDecision("0.1.0", "0.2.0")).toEqual({ show: true, store: null });
  });

  it("does nothing while the version is unknown", () => {
    expect(whatsNewDecision(null, "")).toEqual({ show: false, store: null });
  });
});

describe("stored values", () => {
  it("reads and writes through the given storage", () => {
    const data = new Map<string, string>();
    const storage = {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => void data.set(key, value),
    };
    writeStored("key", "value", storage);
    expect(readStored("key", storage)).toBe("value");
  });

  it("treats broken storage as empty", () => {
    const broken = {
      getItem: () => {
        throw new Error("blocked");
      },
    };
    expect(readStored("key", broken)).toBeNull();
  });
});
