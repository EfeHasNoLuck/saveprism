import { describe, expect, it } from "vitest";
import { findJsonProblem } from "./jsonCheck";

describe("findJsonProblem", () => {
  it("accepts valid JSON of every kind", () => {
    for (const text of [
      '{"a": [1, -2.5e3, true, false, null, "x\\n\\u00e7"], "b": {}}',
      "[]",
      " 0 ",
      '"text"',
      '{\n  "deep": [[[[{"k": "v"}]]]]\n}',
    ]) {
      expect(findJsonProblem(text), text).toBeNull();
      expect(() => JSON.parse(text)).not.toThrow();
    }
  });

  it("names the problem and where it is", () => {
    const cases: [string, string, number][] = [
      ['{"a": 1,}', "expectedKey", 8],
      ["[1, ]", "unexpected", 4],
      ['{"a" 1}', "expectedColon", 5],
      ['{"a": 1 "b": 2}', "expectedCommaOrBrace", 8],
      ["[1 2]", "expectedCommaOrBracket", 3],
      ['{"a": "x', "unclosedString", 6],
      ['"a\tb"', "controlChar", 2],
      ['"\\q"', "badEscape", 1],
      ['"\\u12"', "badEscape", 1],
      ["[01]", "badNumber", 1],
      ["[1.]", "badNumber", 1],
      ["-", "badNumber", 0],
      ["{} x", "trailing", 3],
      ['{"a": ', "end", 6],
      ["", "end", 0],
      ["[tru]", "unexpected", 1],
    ];
    for (const [text, kind, offset] of cases) {
      expect(findJsonProblem(text), text).toEqual({ kind, offset });
      expect(() => JSON.parse(text), text).toThrow();
    }
  });

  it("handles nesting deeper than any call stack", () => {
    const deep = "[".repeat(100_000) + "]".repeat(100_000);
    expect(findJsonProblem(deep)).toBeNull();
    expect(findJsonProblem("[".repeat(100_000))).toEqual({ kind: "end", offset: 100_000 });
  });
});
