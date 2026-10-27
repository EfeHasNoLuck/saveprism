/**
 * Keyboard shortcuts of the app: the bindings the player chose, how a key press is matched against
 * them, and the rules a new combination must follow. No DOM and no React here.
 */

export type ShortcutAction =
  | "open"
  | "pasteCode"
  | "save"
  | "saveAs"
  | "closeFile"
  | "undo"
  | "redo"
  | "find"
  | "undoAll"
  | "openSettings"
  | "whereSaves";

export type ShortcutGroup = "file" | "editing" | "app";

/** Every action, in the order the Settings tab lists them. */
export const SHORTCUT_ACTIONS: readonly { action: ShortcutAction; group: ShortcutGroup }[] = [
  { action: "open", group: "file" },
  { action: "pasteCode", group: "file" },
  { action: "save", group: "file" },
  { action: "saveAs", group: "file" },
  { action: "closeFile", group: "file" },
  { action: "undo", group: "editing" },
  { action: "redo", group: "editing" },
  { action: "find", group: "editing" },
  { action: "undoAll", group: "editing" },
  { action: "openSettings", group: "app" },
  { action: "whereSaves", group: "app" },
];

/** The shortcuts of each action: 0, 1 or 2 combinations (canonical, for example `Ctrl+Shift+S`). */
export type ShortcutBindings = Record<ShortcutAction, string[]>;

/** More than this many shortcuts per action are not stored. */
export const MAX_SHORTCUTS_PER_ACTION = 2;
/** At most two modifiers and one other key. */
export const MAX_KEYS = 3;

const DEFAULT_SHORTCUTS: ShortcutBindings = {
  open: ["Ctrl+O"],
  pasteCode: [],
  save: ["Ctrl+S"],
  saveAs: ["Ctrl+Shift+S"],
  closeFile: [],
  undo: ["Ctrl+Z"],
  redo: ["Ctrl+Y", "Ctrl+Shift+Z"],
  find: ["Ctrl+F"],
  undoAll: [],
  openSettings: [],
  whereSaves: [],
};

/** A fresh copy of the defaults, safe to store in the settings state. */
export function defaultShortcuts(): ShortcutBindings {
  return cloneShortcuts(DEFAULT_SHORTCUTS);
}

function cloneShortcuts(bindings: ShortcutBindings): ShortcutBindings {
  return {
    open: [...bindings.open],
    pasteCode: [...bindings.pasteCode],
    save: [...bindings.save],
    saveAs: [...bindings.saveAs],
    closeFile: [...bindings.closeFile],
    undo: [...bindings.undo],
    redo: [...bindings.redo],
    find: [...bindings.find],
    undoAll: [...bindings.undoAll],
    openSettings: [...bindings.openSettings],
    whereSaves: [...bindings.whereSaves],
  };
}

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

/** One parsed combination: the modifiers plus the key that is not Ctrl, Shift or Alt. */
export interface ParsedCombination {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  key: string;
}

/** Why a combination cannot be used. `null` means it is fine. */
export type CombinationProblem =
  | "threeKeys"
  | "needCtrlOrAlt"
  | "ctrlAlt"
  | "system"
  | "reserved"
  | "unknown";

const PUNCTUATION = new Set([",", ".", "/", ";", "'", "[", "]", "\\", "-", "=", "`"]);

const NAMED_KEYS: Record<string, string> = {
  space: "Space",
  spacebar: "Space",
  tab: "Tab",
  escape: "Escape",
  esc: "Escape",
  enter: "Enter",
  return: "Enter",
  backspace: "Backspace",
  delete: "Delete",
  del: "Delete",
  insert: "Insert",
  home: "Home",
  end: "End",
  pageup: "PageUp",
  pagedown: "PageDown",
  arrowup: "ArrowUp",
  up: "ArrowUp",
  arrowdown: "ArrowDown",
  down: "ArrowDown",
  arrowleft: "ArrowLeft",
  left: "ArrowLeft",
  arrowright: "ArrowRight",
  right: "ArrowRight",
};

const CODE_KEYS: Record<string, string> = {
  Comma: ",",
  Period: ".",
  Slash: "/",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Minus: "-",
  Equal: "=",
  Backquote: "`",
  Space: "Space",
  Tab: "Tab",
  Escape: "Escape",
  Enter: "Enter",
  Backspace: "Backspace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
};

const FUNCTION_KEY = /^F([1-9]|1[0-2])$/i;

const SYSTEM_COMBINATIONS = new Set([
  "Alt+F4",
  "Alt+Tab",
  "Alt+Escape",
  "Alt+Space",
  "Ctrl+Escape",
  "Ctrl+Shift+Escape",
]);

function canonicalKey(token: string): string | null {
  const trimmed = token.trim();
  if (trimmed === "" || trimmed === " ") return null;
  const lower = trimmed.toLowerCase();
  const named = NAMED_KEYS[lower];
  if (named !== undefined) return named;
  if (/^[a-z]$/i.test(trimmed)) return trimmed.toUpperCase();
  if (/^[0-9]$/.test(trimmed)) return trimmed;
  if (FUNCTION_KEY.test(trimmed)) return trimmed.toUpperCase();
  if (PUNCTUATION.has(trimmed)) return trimmed;
  return null;
}

/** The key of a key press: from `key`, or from the physical key on a non-Latin layout. */
function keyOf(input: KeyInput): string | null {
  const direct = canonicalKey(input.key);
  if (direct !== null) return direct;
  const code = input.code ?? "";
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1] ?? null;
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1] ?? null;
  if (FUNCTION_KEY.test(code)) return code;
  return CODE_KEYS[code] ?? null;
}

/** The combination of a key press, or `null` when it has no usable key (Enter and Esc included). */
export function combinationFromInput(input: KeyInput): ParsedCombination | null {
  const key = keyOf(input);
  if (key === null || key === "Enter" || key === "Escape") return null;
  return { ctrl: input.ctrlKey || input.metaKey, shift: input.shiftKey, alt: input.altKey, key };
}

/** The canonical text of a combination (`Ctrl+Shift+S`). */
export function formatCombination(combination: ParsedCombination): string {
  const parts: string[] = [];
  if (combination.ctrl) parts.push("Ctrl");
  if (combination.shift) parts.push("Shift");
  if (combination.alt) parts.push("Alt");
  parts.push(combination.key);
  return parts.join("+");
}

/**
 * Parses stored or typed text into a combination. Returns `null` for text that is not one
 * combination of modifiers plus a key.
 */
export function parseCombination(text: string): ParsedCombination | null {
  const tokens = text.split("+").map((token) => token.trim());
  if (tokens.length === 0 || tokens.some((token) => token === "")) return null;
  const combination: ParsedCombination = { ctrl: false, shift: false, alt: false, key: "" };
  for (const token of tokens) {
    const lower = token.toLowerCase();
    if (lower === "ctrl" || lower === "control" || lower === "cmd" || lower === "command" || lower === "meta") {
      combination.ctrl = true;
      continue;
    }
    if (lower === "shift") {
      combination.shift = true;
      continue;
    }
    if (lower === "alt" || lower === "option") {
      combination.alt = true;
      continue;
    }
    const key = canonicalKey(token);
    if (key === null || combination.key !== "") return null;
    combination.key = key;
  }
  return combination.key === "" ? null : combination;
}

/** Checks a combination against the app's rules; `null` when nothing is wrong. */
export function validateCombination(combination: ParsedCombination): CombinationProblem | null {
  if (SYSTEM_COMBINATIONS.has(formatCombination(combination))) return "system";
  if (combination.key === "Enter" || combination.key === "Escape") return "reserved";
  const modifiers = Number(combination.ctrl) + Number(combination.shift) + Number(combination.alt);
  if (modifiers > 2) return "threeKeys";
  if (combination.ctrl && combination.alt) return "ctrlAlt";
  if (!combination.ctrl && !combination.alt && !FUNCTION_KEY.test(combination.key)) {
    return "needCtrlOrAlt";
  }
  return null;
}

/** Checks canonical or typed text; `null` when it is usable. */
export function validateCombinationText(text: string): CombinationProblem | null {
  const combination = parseCombination(text);
  return combination === null ? "unknown" : validateCombination(combination);
}

/**
 * The action that already uses `combination`, or `null`. The recording dialog moves the
 * combination from that action to the one being edited.
 */
export function conflictingAction(
  bindings: ShortcutBindings,
  action: ShortcutAction,
  combination: string,
): ShortcutAction | null {
  for (const { action: other } of SHORTCUT_ACTIONS) {
    if (other !== action && bindings[other].includes(combination)) return other;
  }
  return null;
}

function withoutOtherActions(
  bindings: ShortcutBindings,
  action: ShortcutAction,
  combination: string,
): ShortcutBindings {
  const next = cloneShortcuts(bindings);
  for (const { action: other } of SHORTCUT_ACTIONS) {
    if (other === action) continue;
    next[other] = next[other].filter((item) => item !== combination);
  }
  return next;
}

/**
 * Puts `combination` on `action`, moving it away from any other action. With `replace` it takes
 * that slot of the action; without it, it is added while the action has fewer than two slots.
 * An action that already holds two shortcuts and is not replacing one stays unchanged.
 */
export function withShortcut(
  bindings: ShortcutBindings,
  action: ShortcutAction,
  combination: string,
  replace?: number,
): ShortcutBindings {
  const current = bindings[action];
  const replacing =
    replace !== undefined && Number.isInteger(replace) && replace >= 0 && replace < current.length;
  if (!replacing && current.includes(combination)) return cloneShortcuts(bindings);
  if (!replacing && current.length >= MAX_SHORTCUTS_PER_ACTION) return cloneShortcuts(bindings);

  const next = withoutOtherActions(bindings, action, combination);
  const target = [...current];
  if (replacing && replace !== undefined) {
    target[replace] = combination;
  } else {
    target.push(combination);
  }
  next[action] = [...new Set(target)].slice(0, MAX_SHORTCUTS_PER_ACTION);
  return next;
}

/** Removes one combination from an action; the other actions stay as they are. */
export function removeShortcut(
  bindings: ShortcutBindings,
  action: ShortcutAction,
  combination: string,
): ShortcutBindings {
  const next = cloneShortcuts(bindings);
  next[action] = next[action].filter((item) => item !== combination);
  return next;
}

/** Puts one action back to its defaults. */
export function resetShortcut(bindings: ShortcutBindings, action: ShortcutAction): ShortcutBindings {
  const next = cloneShortcuts(bindings);
  next[action] = [...DEFAULT_SHORTCUTS[action]];
  return next;
}

/** Puts every action back to its defaults. */
export function resetAllShortcuts(): ShortcutBindings {
  return defaultShortcuts();
}

/**
 * Reads stored bindings. An action missing from the stored object, or holding a value that is not
 * a list of valid combinations, falls back to its default; an empty list means "Not set".
 */
export function parseStoredShortcuts(raw: unknown): ShortcutBindings {
  const next = defaultShortcuts();
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return next;
  const record = raw as Record<string, unknown>;
  for (const { action } of SHORTCUT_ACTIONS) {
    if (!Object.prototype.hasOwnProperty.call(record, action)) continue;
    const value = record[action];
    if (!Array.isArray(value)) continue;
    if (value.length === 0) {
      next[action] = [];
      continue;
    }
    if (value.length > MAX_SHORTCUTS_PER_ACTION) continue;
    const parsed: string[] = [];
    let broken = false;
    for (const item of value) {
      const combination = typeof item === "string" ? parseCombination(item) : null;
      if (combination === null || validateCombination(combination) !== null) {
        broken = true;
        break;
      }
      const formatted = formatCombination(combination);
      if (!parsed.includes(formatted)) parsed.push(formatted);
    }
    if (broken || parsed.length === 0) continue;
    next[action] = parsed;
  }
  return next;
}

/**
 * The action of a key press, or `null` when it is not one of the bound shortcuts. The rules are
 * checked too, so a stored system combination can never run.
 */
export function shortcutFor(input: KeyInput, bindings: ShortcutBindings = DEFAULT_SHORTCUTS): ShortcutAction | null {
  const combination = combinationFromInput(input);
  if (combination === null) return null;
  if (validateCombination(combination) !== null) return null;
  const text = formatCombination(combination);
  for (const { action } of SHORTCUT_ACTIONS) {
    if (bindings[action].includes(text)) return action;
  }
  return null;
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
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
    if (target instanceof HTMLInputElement && !TEXT_INPUT_TYPES.has(target.type)) return false;
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
