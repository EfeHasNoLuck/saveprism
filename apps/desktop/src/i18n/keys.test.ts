// @vitest-environment node
import { describe, expect, it } from "vitest";
import enCommon from "../../locales/en/common.json";
import enEditor from "../../locales/en/editor.json";
import trCommon from "../../locales/tr/common.json";
import trEditor from "../../locales/tr/editor.json";

const LOCALES = {
  en: { common: enCommon, editor: enEditor },
  tr: { common: trCommon, editor: trEditor },
} as const;

type Bundle = Record<string, unknown>;

const PLURAL_SUFFIXES = ["_zero", "_one", "_two", "_few", "_many", "_other"];

/** Every app source file as text, keyed by its path under `src/`. */
const SOURCES = import.meta.glob("../**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

function lookup(bundle: unknown, parts: string[]): unknown {
  let node = bundle;
  for (const part of parts) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Bundle)[part];
  }
  return node;
}

function isPluralObject(value: unknown): boolean {
  return (
    typeof value === "object" &&
    value !== null &&
    PLURAL_SUFFIXES.some((suffix) => typeof (value as Bundle)[suffix] === "string")
  );
}

/** True when `key` resolves to a string or to i18next's flattened plural forms (`_one`/`_other`). */
function hasKey(bundle: Bundle, key: string): boolean {
  const value = lookup(bundle, key.split("."));
  if (typeof value === "string" || isPluralObject(value)) return true;
  if (PLURAL_SUFFIXES.some((suffix) => typeof lookup(bundle, `${key}${suffix}`.split(".")) === "string")) {
    return true;
  }
  const suffix = PLURAL_SUFFIXES.find((item) => key.endsWith(item));
  if (suffix === undefined) return false;
  const base = lookup(bundle, key.slice(0, -suffix.length).split("."));
  return isPluralObject(base);
}

const LITERAL_CALL = /\b(?:t|tc)\s*\(\s*["']([^"']+)["']/g;

describe("literal translation keys", () => {
  it("exist in both locales and in the namespace the file loads", () => {
    const missing: string[] = [];

    for (const [path, source] of Object.entries(SOURCES)) {
      if (/\.test\.(ts|tsx)$/.test(path)) continue;
      const namespaces = ["common"];
      if (
        path.startsWith("../editor/") ||
        /useTranslation\(\s*["']editor["']\s*\)/.test(source)
      ) {
        namespaces.push("editor");
      }

      for (const match of source.matchAll(LITERAL_CALL)) {
        const key = match[1];
        if (key === undefined || !/^[A-Za-z][A-Za-z0-9_.-]*$/.test(key)) continue;
        for (const language of ["en", "tr"] as const) {
          const bundles = namespaces.map(
            (namespace) => LOCALES[language][namespace as "common" | "editor"] as Bundle,
          );
          if (!bundles.some((bundle) => hasKey(bundle, key))) {
            missing.push(`${path}: ${language} ${key}`);
          }
        }
      }
    }

    expect(missing).toEqual([]);
  });
});
