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
