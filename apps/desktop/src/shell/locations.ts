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

/**
 * True when the host path still holds one of its placeholders (`<game folder>`, `<game>`,
 * `<company>`). Such a path depends on the game, so the host cannot say whether it exists.
 */
export function hasPathPlaceholder(path: string): boolean {
  return ["<game folder>", "<game>", "<company>"].some((placeholder) =>
    path.includes(placeholder),
  );
}
