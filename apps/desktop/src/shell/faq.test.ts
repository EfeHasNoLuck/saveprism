// @vitest-environment node
import { describe, expect, it } from "vitest";
import en from "../../locales/en/common.json";
import tr from "../../locales/tr/common.json";
import { FAQ_ITEMS } from "./faq";

function read(source: Record<string, unknown>, key: string): unknown {
  return key.split(".").reduce<unknown>((value, part) => {
    if (value === null || typeof value !== "object") return undefined;
    return (value as Record<string, unknown>)[part];
  }, source);
}

describe("start screen FAQ", () => {
  it("has the five questions (Where is my save? lives on the drop card)", () => {
    expect(FAQ_ITEMS.map((item) => item.id)).toEqual([
      "original",
      "games",
      "notOpen",
      "ai",
      "undo",
    ]);
  });

  it("has a non-empty question and answer in English and Turkish", () => {
    for (const item of FAQ_ITEMS) {
      for (const source of [en, tr]) {
        const question = read(source, item.questionKey);
        const answer = read(source, item.answerKey);
        expect(typeof question, item.questionKey).toBe("string");
        expect(typeof answer, item.answerKey).toBe("string");
        expect(String(question).trim(), item.questionKey).not.toBe("");
        expect(String(answer).trim(), item.answerKey).not.toBe("");
      }
    }
  });
});
