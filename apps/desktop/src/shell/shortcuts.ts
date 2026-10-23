/** Keyboard shortcuts of the app: which key does what, and where typing owns the keys. */

export type ShortcutAction = "open" | "save" | "saveAs" | "undo" | "redo" | "find";

/** The parts of a keyboard event the shortcuts look at. */
export interface KeyInput {
  key: string;
  /** Physical key ("KeyZ"); used when the layout has no Latin letter on it. */
  code?: string;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
}

/** The letter of a key press: from `key`, or from the physical key on a non-Latin layout. */
function letterOf(input: KeyInput): string {
  if (/^[a-z]$/i.test(input.key)) return input.key.toLowerCase();
  const physical = /^Key([A-Z])$/.exec(input.code ?? "");
  return physical ? (physical[1] ?? "").toLowerCase() : input.key.toLowerCase();
}

/**
 * The action of a key press, or `null` when it is not a shortcut. Ctrl (or Cmd) plus a letter;
 * Alt is refused because AltGr is Ctrl+Alt on many layouts and types characters there.
 */
export function shortcutFor(input: KeyInput): ShortcutAction | null {
  if (input.altKey || !(input.ctrlKey || input.metaKey)) return null;
  switch (letterOf(input)) {
    case "o":
      return input.shiftKey ? null : "open";
    case "s":
      return input.shiftKey ? "saveAs" : "save";
    case "z":
      return input.shiftKey ? "redo" : "undo";
    case "y":
      return input.shiftKey ? null : "redo";
    case "f":
      return input.shiftKey ? null : "find";
    default:
      return null;
  }
}

const TEXT_INPUT_TYPES = new Set(["", "text", "search", "email", "url", "tel", "password", "number"]);

/**
 * True when `target` is a place where Ctrl+Z and Ctrl+Y belong to the text itself: a text box,
 * a text area, the JSON editor. A value box whose text is the committed value (`data-committed`)
 * has nothing typed to undo, so the undo of the edits applies there.
 */
export function isTextEditing(target: Element | null): boolean {
  if (target === null) return false;
  if (target.closest(".cm-editor") !== null) return true;
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLInputElement) {
    if (!TEXT_INPUT_TYPES.has(target.type)) return false;
    const committed = target.dataset.committed;
    return committed === undefined || committed !== target.value;
  }
  return target instanceof HTMLElement && target.isContentEditable;
}

/**
 * The search box of the first list card on screen: the first one (in page order) that is inside
 * the visible part of `root`, else the first one. `null` when no list has a search box.
 */
export function firstListSearch(root: Element): HTMLInputElement | null {
  const boxes = Array.from(root.querySelectorAll<HTMLInputElement>("input[data-list-search]"));
  const view = root.getBoundingClientRect();
  const visible = boxes.find((box) => {
    const rect = box.getBoundingClientRect();
    return rect.bottom > view.top && rect.top < view.bottom;
  });
  return visible ?? boxes[0] ?? null;
}
