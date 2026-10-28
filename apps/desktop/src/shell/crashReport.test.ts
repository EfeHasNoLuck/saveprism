// @vitest-environment node
import { describe, expect, it } from "vitest";
import en from "../../locales/en/common.json";
import tr from "../../locales/tr/common.json";
import {
  buildCrashReport,
  CRASH_REPORT_TITLE,
  CRASH_TEXT_FALLBACK,
  describeError,
  stripPaths,
} from "./crashReport";

describe("stripPaths: Windows paths", () => {
  it("removes a drive path with backslashes or forward slashes", () => {
    expect(stripPaths(String.raw`Cannot open C:\Users\ada\Saves\slot1.sav`)).toBe("Cannot open <path>");
    expect(stripPaths("Cannot open C:/Users/ada/Saves/slot1.sav")).toBe("Cannot open <path>");
    expect(stripPaths(String.raw`d:\games\save.dat`)).toBe("<path>");
  });

  it("removes a path with spaces, parentheses and an apostrophe in a folder name", () => {
    expect(stripPaths(String.raw`at C:\Users\Ada Lovelace\My Saves\slot 1.sav`)).toBe("at <path>");
    expect(stripPaths(String.raw`C:\Program Files (x86)\Game\save.dat`)).toBe("<path>");
    expect(stripPaths(String.raw`C:\Users\O'Brien\save.dat`)).toBe("<path>");
  });

  it("stops where the sentence goes on after a colon, comma or period", () => {
    expect(stripPaths(String.raw`Could not read C:\Users\Ada Lovelace\save.dat: access denied`)).toBe(
      "Could not read <path>: access denied",
    );
    expect(stripPaths(String.raw`Failed on C:\a\b.sav, retrying`)).toBe("Failed on <path>, retrying");
    expect(stripPaths(String.raw`Failed on C:\a\b.sav. Reload the app.`)).toBe("Failed on <path>. Reload the app.");
    expect(stripPaths(String.raw`Failed on C:\a\b.sav.`)).toBe("Failed on <path>.");
  });

  it("stops at a quote or a closing bracket", () => {
    expect(stripPaths(String.raw`cannot read "C:\Users\Ada Lovelace\a b.sav" now`)).toBe('cannot read "<path>" now');
    expect(stripPaths(String.raw`(reading 'C:\Users\ada\x.sav')`)).toBe("(reading '<path>')");
    expect(stripPaths(String.raw`at Foo (C:\Users\ada\app\Foo.tsx:12:5)`)).toBe("at Foo (<path>:12:5)");
  });

  it("removes UNC, extended-length and environment-variable paths", () => {
    expect(stripPaths(String.raw`\\server\share\saves\slot1.sav`)).toBe("<path>");
    expect(stripPaths(String.raw`\\?\C:\Users\ada\x.sav`)).toBe("<path>");
    expect(stripPaths(String.raw`%APPDATA%\SavePrism\backups\x.sav`)).toBe("<path>");
    expect(stripPaths(String.raw`C:\\Users\\ada\\x.sav`)).toBe("<path>");
  });

  it("keeps the line and column of a position inside the app", () => {
    expect(stripPaths(String.raw`C:\build\app\index.js:1:2345`)).toBe("<path>:1:2345");
  });
});

describe("stripPaths: URLs", () => {
  it("removes file, asset, tauri and ipc URLs", () => {
    expect(stripPaths("at file:///C:/Users/ada/app/index.html:3:1")).toBe("at <path>:3:1");
    expect(stripPaths("asset://localhost/C%3A%5CUsers%5Cada%5Cx.png failed")).toBe("<path> failed");
    expect(stripPaths("tauri://localhost/assets/index-abc.js:1:2345")).toBe("<path>:1:2345");
    expect(stripPaths("ipc://localhost/read_file")).toBe("<path>");
  });

  it("removes dev-server and packaged-app localhost URLs, with a port and a query", () => {
    expect(stripPaths("at App (http://localhost:1420/src/App.tsx?t=17:120:5)")).toBe("at App (<path>:120:5)");
    expect(stripPaths("at App (http://127.0.0.1:1420/src/App.tsx:1:2)")).toBe("at App (<path>:1:2)");
    expect(stripPaths("at x (http://tauri.localhost/assets/index-abc.js:1:2345)")).toBe("at x (<path>:1:2345)");
    expect(stripPaths("https://asset.localhost/x.png")).toBe("<path>");
    expect(stripPaths("http://localhost:1420")).toBe("<path>");
  });

  it("leaves other web addresses alone", () => {
    const text = "visit https://react.dev/errors/31?args[]=object for the full message";
    expect(stripPaths(text)).toBe(text);
  });
});

describe("stripPaths: POSIX and relative paths", () => {
  it("removes absolute, home and dot-relative paths", () => {
    expect(stripPaths("Cannot open /home/ada/saves/slot1.sav")).toBe("Cannot open <path>");
    expect(stripPaths("Cannot open /Users/Ada Lovelace/Saves/slot1.sav")).toBe("Cannot open <path>");
    expect(stripPaths("Cannot open ~/saves/slot1.sav")).toBe("Cannot open <path>");
    expect(stripPaths("Cannot open ./saves/slot1.sav and ../x")).toBe("Cannot open <path>");
    expect(stripPaths("Cannot open //server/share/slot1.sav")).toBe("Cannot open <path>");
    expect(stripPaths("(reading '/etc/hosts')")).toBe("(reading '<path>')");
  });

  it("removes a relative path that ends in a file name", () => {
    expect(stripPaths("at src/App.tsx:12:5")).toBe("at <path>:12:5");
    expect(stripPaths(String.raw`saves\slot1.sav is broken`)).toBe("<path> is broken");
    expect(stripPaths("node_modules/react-dom/cjs/react-dom-client.development.js")).toBe("<path>");
  });
});

describe("stripPaths: ordinary text", () => {
  it.each([
    "Cannot read properties of undefined (reading 'name')",
    "Objects are not valid as a React child (found: object with keys {a, b})",
    "Use read/write access, and/or N/A, TCP/IP, text/html, application/json",
    "On 10/28/2026 at 12:30 the value was 3 / 4, about 50%",
    "Unexpected token < in JSON at position 0: <div>x</div> and <br />",
    "Node.js, React.StrictMode and v1.2/v1.3 are fine",
    "a:b and a: b and 12:30",
    "",
  ])("is left as it is: %s", (text) => {
    expect(stripPaths(text)).toBe(text);
  });
});

describe("stripPaths: a whole component stack", () => {
  it("keeps the component names and drops every location", () => {
    const stack = [
      "",
      "    at DevCrashTrigger (http://localhost:1420/src/shell/devCrash.tsx?t=1730000000:9:11)",
      "    at CrashBoundary (http://localhost:1420/src/shell/CrashBoundary.tsx:30:5)",
      "    at StrictMode (<anonymous>)",
      String.raw`    at App (C:\Users\Ada Lovelace\dev\saveprism\src\App.tsx:1:1)`,
    ].join("\n");
    expect(stripPaths(stack)).toBe(
      [
        "",
        "    at DevCrashTrigger (<path>:9:11)",
        "    at CrashBoundary (<path>:30:5)",
        "    at StrictMode (<anonymous>)",
        "    at App (<path>:1:1)",
      ].join("\n"),
    );
  });
});

describe("describeError", () => {
  it("reads an Error", () => {
    expect(describeError(new TypeError("boom"))).toEqual({ name: "TypeError", message: "boom" });
  });

  it("reads anything else that was thrown", () => {
    expect(describeError("boom")).toEqual({ name: "Error", message: "boom" });
    expect(describeError(42)).toEqual({ name: "Error", message: "42" });
    expect(describeError(undefined)).toEqual({ name: "Error", message: "undefined" });
  });

  it("survives a value that cannot be turned into text", () => {
    const hostile = {
      toString() {
        throw new Error("no");
      },
    };
    expect(describeError(hostile).message).toMatch(/cannot be shown/);
  });
});

describe("buildCrashReport", () => {
  const date = new Date("2026-10-28T12:34:56.000Z");

  it("holds the version, the date, the error and the component stack", () => {
    const text = buildCrashReport({
      version: "0.1.0",
      name: "TypeError",
      message: "x is undefined",
      componentStack: "\n    at Foo\n    at Bar\n",
      date,
    });
    expect(text).toBe(
      [
        CRASH_REPORT_TITLE,
        "- SavePrism version: 0.1.0",
        "- Date: 2026-10-28T12:34:56.000Z",
        "",
        "Error:",
        "TypeError: x is undefined",
        "",
        "React component stack:",
        "    at Foo",
        "    at Bar",
      ].join("\n"),
    );
  });

  it("holds no path or file name, whether it is in the message, the name or the stack", () => {
    const text = buildCrashReport({
      version: "0.1.0",
      name: "Error",
      message: String.raw`Could not read C:\Users\Ada Lovelace\Saves\slot1.sav: nope (http://localhost:1420/src/x.ts:1:1)`,
      componentStack: [
        "",
        "    at Foo (http://tauri.localhost/assets/index-abc.js:1:2345)",
        String.raw`    at Bar (C:\Users\Ada Lovelace\app\Bar.tsx:2:3)`,
      ].join("\n"),
      date,
    });
    expect(text).not.toMatch(/Ada|Lovelace|slot1|Saves|index-abc|Bar\.tsx|localhost|tauri\.localhost|C:/);
    expect(text).toContain("Could not read <path>: nope (<path>:1:1)");
    expect(text).toContain("    at Foo (<path>:1:2345)");
    expect(text).toContain("    at Bar (<path>:2:3)");
  });

  it("copes with a missing stack and an empty message", () => {
    const text = buildCrashReport({ version: "unknown", name: "", message: "", componentStack: "", date });
    expect(text).toContain("- SavePrism version: unknown");
    expect(text).toContain("Error:\nError\n");
    expect(text.endsWith("React component stack:\n(not available)")).toBe(true);
  });
});

describe("crash text", () => {
  it("the English fallback equals the English locale, and Turkish has the same keys", () => {
    expect(en.crash).toEqual(CRASH_TEXT_FALLBACK);
    expect(Object.keys(tr.crash).sort()).toEqual(Object.keys(CRASH_TEXT_FALLBACK).sort());
    for (const value of Object.values(tr.crash)) {
      expect(value.trim()).not.toBe("");
    }
  });
});
