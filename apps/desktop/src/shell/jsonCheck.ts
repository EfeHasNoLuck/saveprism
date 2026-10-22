/**
 * A strict JSON syntax check that says where the text goes wrong, so the editor can show
 * "Line 12, column 5: Expected ',' or '}'." before anything is sent to the host. It only scans the
 * text (no values are built) and keeps its own stack, so deep nesting cannot overflow it.
 */

export type JsonProblemKind =
  | "end"
  | "unexpected"
  | "expectedKey"
  | "expectedColon"
  | "expectedCommaOrBrace"
  | "expectedCommaOrBracket"
  | "unclosedString"
  | "controlChar"
  | "badEscape"
  | "badNumber"
  | "trailing";

export interface JsonProblem {
  /** Offset of the problem in the text (UTF-16 units). */
  offset: number;
  kind: JsonProblemKind;
}

const OBJECT = 1;
const ARRAY = 2;

function isSpace(code: number): boolean {
  return code === 0x20 || code === 0x0a || code === 0x0d || code === 0x09;
}

function isDigit(code: number): boolean {
  return code >= 0x30 && code <= 0x39;
}

function isHex(code: number): boolean {
  return isDigit(code) || (code >= 0x41 && code <= 0x46) || (code >= 0x61 && code <= 0x66);
}

/** The first syntax problem of `text`, or `null` when it is valid JSON. */
export function findJsonProblem(text: string): JsonProblem | null {
  const n = text.length;
  let i = 0;
  const stack: number[] = [];
  const problem = (kind: JsonProblemKind, at = i): JsonProblem => ({
    offset: Math.min(at, n),
    kind,
  });
  const skipSpace = () => {
    while (i < n && isSpace(text.charCodeAt(i))) i += 1;
  };

  /** A string starting at the quote under `i`; leaves `i` after the closing quote. */
  const scanString = (): JsonProblem | null => {
    const start = i;
    i += 1;
    while (i < n) {
      const code = text.charCodeAt(i);
      if (code === 0x22) {
        i += 1;
        return null;
      }
      if (code === 0x5c) {
        const escape = text.charCodeAt(i + 1);
        if (escape === 0x75) {
          for (let k = 2; k < 6; k += 1) {
            if (!isHex(text.charCodeAt(i + k))) return problem("badEscape");
          }
          i += 6;
        } else if (i + 1 < n && '"\\/bfnrt'.includes(text.charAt(i + 1))) {
          i += 2;
        } else {
          return problem(i + 1 >= n ? "unclosedString" : "badEscape", i + 1 >= n ? start : i);
        }
      } else if (code < 0x20) {
        return problem("controlChar");
      } else {
        i += 1;
      }
    }
    return problem("unclosedString", start);
  };

  const scanDigits = (): boolean => {
    const start = i;
    while (i < n && isDigit(text.charCodeAt(i))) i += 1;
    return i > start;
  };

  /** -? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)? */
  const scanNumber = (): JsonProblem | null => {
    const start = i;
    if (text.charCodeAt(i) === 0x2d) i += 1;
    if (text.charCodeAt(i) === 0x30) {
      i += 1;
    } else if (!scanDigits()) {
      return problem("badNumber", start);
    }
    if (text.charCodeAt(i) === 0x2e) {
      i += 1;
      if (!scanDigits()) return problem("badNumber", start);
    }
    const e = text.charCodeAt(i);
    if (e === 0x65 || e === 0x45) {
      i += 1;
      const sign = text.charCodeAt(i);
      if (sign === 0x2b || sign === 0x2d) i += 1;
      if (!scanDigits()) return problem("badNumber", start);
    }
    // "01" or "1x": a number must end at a delimiter.
    const after = text.charCodeAt(i);
    if (i < n && (isDigit(after) || /[A-Za-z_.]/.test(text.charAt(i)))) {
      return problem("badNumber", start);
    }
    return null;
  };

  /** A key and its colon after `{` or `,` inside an object. */
  const scanKey = (): JsonProblem | null => {
    skipSpace();
    if (i >= n) return problem("end");
    if (text.charCodeAt(i) !== 0x22) return problem("expectedKey");
    const bad = scanString();
    if (bad) return bad;
    skipSpace();
    if (i >= n) return problem("end");
    if (text.charCodeAt(i) !== 0x3a) return problem("expectedColon");
    i += 1;
    return null;
  };

  for (;;) {
    // A value is expected at `i`.
    skipSpace();
    if (i >= n) return problem("end");
    const code = text.charCodeAt(i);
    let opened = false;
    if (code === 0x7b) {
      i += 1;
      skipSpace();
      if (text.charCodeAt(i) === 0x7d) {
        i += 1;
      } else {
        stack.push(OBJECT);
        const bad = scanKey();
        if (bad) return bad;
        opened = true;
      }
    } else if (code === 0x5b) {
      i += 1;
      skipSpace();
      if (text.charCodeAt(i) === 0x5d) {
        i += 1;
      } else {
        stack.push(ARRAY);
        opened = true;
      }
    } else if (code === 0x22) {
      const bad = scanString();
      if (bad) return bad;
    } else if (code === 0x2d || isDigit(code)) {
      const bad = scanNumber();
      if (bad) return bad;
    } else {
      const word = text.startsWith("true", i)
        ? "true"
        : text.startsWith("false", i)
          ? "false"
          : text.startsWith("null", i)
            ? "null"
            : null;
      if (word === null) return problem("unexpected");
      i += word.length;
    }
    if (opened) continue;

    // After a value: close containers, or a comma for the next value.
    for (;;) {
      skipSpace();
      const top = stack[stack.length - 1];
      if (top === undefined) return i < n ? problem("trailing") : null;
      if (i >= n) return problem("end");
      const next = text.charCodeAt(i);
      if (next === 0x2c) {
        i += 1;
        if (top === OBJECT) {
          const bad = scanKey();
          if (bad) return bad;
        }
        break;
      }
      if ((top === OBJECT && next === 0x7d) || (top === ARRAY && next === 0x5d)) {
        i += 1;
        stack.pop();
        continue;
      }
      return problem(top === OBJECT ? "expectedCommaOrBrace" : "expectedCommaOrBracket");
    }
  }
}
