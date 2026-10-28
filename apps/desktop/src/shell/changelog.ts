/** Pure helpers for `CHANGELOG.md`; the file itself is imported with Vite's `?raw`. */

export interface ChangelogSection {
  version: string;
  lines: string[];
}

/** Every `## <version>` section, in file order. */
export function changelogSections(source: string): ChangelogSection[] {
  const sections: ChangelogSection[] = [];
  let current: ChangelogSection | null = null;
  for (const line of source.split(/\r?\n/)) {
    if (line.startsWith("## ")) {
      const version = line.slice(3).trim();
      current = version === "" ? null : { version, lines: [] };
      if (current !== null) sections.push(current);
    } else if (current !== null) {
      current.lines.push(line);
    }
  }
  for (const section of sections) {
    while (section.lines.length > 0 && section.lines[section.lines.length - 1].trim() === "") {
      section.lines.pop();
    }
  }
  return sections;
}

/** The lines of the section whose heading is `version` (a heading may carry a short suffix). */
export function changelogFor(source: string, version: string): string[] | null {
  const wanted = version.trim();
  if (wanted === "") return null;
  const section = changelogSections(source).find(
    (item) => item.version === wanted || item.version.startsWith(`${wanted} `),
  );
  return section ? section.lines : null;
}

/**
 * The notes of `version` in the UI language, falling back to English when that language has no
 * file or no section for the version. Language tags such as `tr-TR` use the base `tr` file.
 */
export function changelogForLanguage(
  sources: Readonly<Record<string, string>>,
  language: string,
  version: string,
  fallback = "en",
): string[] | null {
  const code = language.trim().toLowerCase().split(/[-_]/)[0] ?? "";
  const localized = code === "" ? undefined : sources[code];
  if (localized !== undefined) {
    const lines = changelogFor(localized, version);
    if (lines !== null) return lines;
  }
  const base = sources[fallback];
  return base === undefined ? null : changelogFor(base, version);
}
