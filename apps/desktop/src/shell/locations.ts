/** Pure helpers of the "Where is my save?" dialog (no React, no Tauri). */

/** The translated names of the path placeholders the host sends. */
export interface PlaceholderLabels {
  gameFolder: string;
  game: string;
  company: string;
}

/**
 * Replaces the host's raw `<game folder>`, `<game>` and `<company>` placeholders with the
 * translated names, keeping the angle brackets (`<Oyun>`). The host text is not changed; this is
 * only for display. A missing label leaves its placeholder as it is.
 */
export function translatePathPlaceholders(path: string, labels: PlaceholderLabels): string {
  const pairs: [string, string][] = [
    ["game folder", labels.gameFolder],
    ["game", labels.game],
    ["company", labels.company],
  ];
  let out = path;
  for (const [placeholder, label] of pairs) {
    const shown = label.trim() === "" ? placeholder : label;
    out = out.split(`<${placeholder}>`).join(`<${shown}>`);
  }
  return out;
}
