// @vitest-environment node
import { describe, expect, it } from "vitest";
import { dismissToastsForTopic, type ToastItem } from "./context";

function toast(id: number, topic?: string): ToastItem {
  return { id, kind: "error", title: `Toast ${id}`, topic };
}

describe("dismissToastsForTopic", () => {
  it("removes every toast of the topic", () => {
    const left = dismissToastsForTopic(
      [toast(1, "open"), toast(2, "save"), toast(3, "open")],
      "open",
    );
    expect(left.map((item) => item.id)).toEqual([2]);
  });

  it("keeps toasts without a topic and of other topics", () => {
    const left = dismissToastsForTopic([toast(1), toast(2, "save")], "open");
    expect(left.map((item) => item.id)).toEqual([1, 2]);
  });

  it("keeps the order of the remaining toasts", () => {
    const left = dismissToastsForTopic(
      [toast(1, "save"), toast(2, "open"), toast(3, "info")],
      "open",
    );
    expect(left.map((item) => item.id)).toEqual([1, 3]);
  });
});
