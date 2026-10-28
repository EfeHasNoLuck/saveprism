/**
 * The text behind the crash screen: the report a player can copy and paste into a GitHub issue, the
 * English fallback strings for when translations are not available, and the path stripping that
 * keeps both free of file names. Nothing here touches the DOM, so it is covered by unit tests.
 */

/** First line of every crash report. */
export const CRASH_REPORT_TITLE = "SavePrism crash report";

/**
 * English text for the crash screen. The screen reads translations first and falls back to these
 * when i18n failed to start, so a broken provider can never leave it blank. A test keeps them equal
 * to `crash` in the English locale file.
 */
export const CRASH_TEXT_FALLBACK = {
  title: "Something went wrong",
  body: "SavePrism hit an unexpected error. Changes you have not saved yet are lost.",
  copy: "Copy report",
  reload: "Reload",
  copied: "Report copied.",
  copyFailed: "Could not copy. Select the text below and copy it by hand.",
} as const;

export type CrashTextKey = keyof typeof CRASH_TEXT_FALLBACK;

/** What every removed path or URL becomes. */
const MASK = "<path>";

/** One character of a path. A path can hold spaces, "(x86)" and an apostrophe ("O'Brien"). */
const PATH_CHAR =
  String.raw`(?:(?![.,;:!](?:\s|$))[^\r\n"'\x60<>|*?()]|'(?=\w)|\([^\r\n"'\x60<>|*?()]*\))`;

/** `scheme://...` up to white space, a quote or a bracket. */
const URL_TOKEN = String.raw`[^\s'"\x60<>()]*`;

/** Paths of the packaged app and the dev server: file, asset, tauri and ipc, plus any localhost host. */
const URL_RE = new RegExp(
  String.raw`\b(?:file|asset|tauri|ipc):\/\/${URL_TOKEN}` +
    String.raw`|\bhttps?:\/\/(?:localhost|127\.0\.0\.1|\[::1\]|[A-Za-z0-9-]+\.localhost)(?::\d+)?(?:\/${URL_TOKEN})?`,
  "gi",
);

/** `C:\...`, `C:/...`, `\\?\C:\...`, `\\server\share\...` and `%APPDATA%\...`. */
const WINDOWS_RE = new RegExp(
  String.raw`(?:(?<![A-Za-z0-9])[A-Za-z]:[\\/]` +
    String.raw`|\\{2,}[?.]\\+` +
    String.raw`|\\{2,}(?=[A-Za-z0-9_$.-]+\\+[A-Za-z0-9_$.-])` +
    String.raw`|%[A-Za-z_][A-Za-z0-9_]*%(?=[\\/]))${PATH_CHAR}*`,
  "gm",
);

/** `/home/me/x`, `~/x`, `./x`, `../x` and `//server/x`; "and/or", "10/28" and "</div>" are left alone. */
const POSIX_RE = new RegExp(
  String.raw`(?<![\w.:/\\)\]<>%$~-])(?:~|\.{1,2})?\/{1,2}(?=[\w.~@%+$-])${PATH_CHAR}*`,
  "gm",
);

/** `src/App.tsx` and `saves\slot1.sav`: a relative path that ends in a file name with an extension. */
const RELATIVE_RE =
  /(?<![\w@~./\\<>-])(?:[\w@~-][\w.@~-]*[\\/])+[\w@~-][\w.@~-]*\.[A-Za-z][A-Za-z0-9]{0,7}(?![\w/\\])/g;

/** A ":12:5" line and column at the end of a match. They point into the bundle, not into the disk. */
const POSITION_TAIL = /(?::\d+){1,2}$/;
/** White space and sentence punctuation after a match are not part of the path. */
const TRAILING = /[\s.,;:!?]+$/;

function mask(match: string, keepPosition: boolean): string {
  const trailing = TRAILING.exec(match)?.[0] ?? "";
  const position = keepPosition
    ? (POSITION_TAIL.exec(match.slice(0, match.length - trailing.length))?.[0] ?? "")
    : "";
  return `${MASK}${position}${trailing}`;
}

/** A URL keeps a ":line:column" only after a path; in "http://localhost:1420" the number is a port. */
function maskUrl(match: string): string {
  return mask(match, /^[a-z][a-z0-9+.-]*:\/\/[^/\s]*\//i.test(match));
}

/**
 * Replaces every file system path and file name in `text` with `<path>`, keeping a trailing
 * `:line:column` because it only points into the app's own code. It reads Windows drive and UNC
 * paths, `/...` paths, relative paths that end in a file name, and `file://`, `asset://`,
 * `tauri://`, `ipc://` and localhost URLs, which is what an error message or a React component
 * stack holds. A path can contain spaces ("C:\Users\Ada Lovelace\..."), so an absolute path runs to
 * the end of its sentence; it errs on the side of removing too much. A bare file name with no folder
 * in front of it (for example "slot1.sav" in prose) is not recognised.
 */
export function stripPaths(text: string): string {
  return text
    .replace(URL_RE, maskUrl)
    .replace(WINDOWS_RE, (match) => mask(match, true))
    .replace(POSIX_RE, (match) => mask(match, true))
    .replace(RELATIVE_RE, (match) => mask(match, true));
}

/** What a caught value looks like to a reader: a name and a message, whatever was thrown. */
export function describeError(error: unknown): { name: string; message: string } {
  if (error instanceof Error) {
    return { name: error.name, message: error.message };
  }
  try {
    return { name: "Error", message: String(error) };
  } catch {
    return { name: "Error", message: "(a value that cannot be shown was thrown)" };
  }
}

export interface CrashReportInput {
  /** The app version, or "unknown" when it could not be read. */
  version: string;
  name: string;
  message: string;
  /** React's component stack for the error; empty when React did not give one. */
  componentStack: string;
  date: Date;
}

/** The text the "Copy report" button puts on the clipboard. Always English, and free of paths. */
export function buildCrashReport(input: CrashReportInput): string {
  const name = stripPaths(input.name).trim() || "Error";
  const message = stripPaths(input.message).trim();
  const stack = stripPaths(input.componentStack)
    .replace(/^(?:[ \t]*\r?\n)+/, "")
    .trimEnd();
  return [
    CRASH_REPORT_TITLE,
    `- SavePrism version: ${input.version}`,
    `- Date: ${input.date.toISOString()}`,
    "",
    "Error:",
    message === "" ? name : `${name}: ${message}`,
    "",
    "React component stack:",
    stack === "" ? "(not available)" : stack,
  ].join("\n");
}
