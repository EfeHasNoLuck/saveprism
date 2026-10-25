/**
 * The text of the "file report" a player can paste into a GitHub issue when SavePrism does not know
 * their file. It is built only from {@link FileReport}, which holds no path and no file name, and it
 * is always English: the people who read the issues read English, whatever the app's language.
 */
import type { EntropyClass, FileReport } from "@/api/commands";

/** First line of every report. */
export const REPORT_TITLE = "SavePrism file report";
/** The line the player completes with the game's name. */
export const GAME_LINE = "- Game: (please write the game name here)";

const ENTROPY_TEXT: Record<EntropyClass, string> = {
  empty: "an empty file",
  text: "text",
  binary: "binary data",
  compressed: "compressed or encrypted data (looks random)",
};

/** 1234567 as "1,234,567" (no locale data needed). */
function groupDigits(value: number): string {
  return String(Math.trunc(value)).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/** A report as Markdown-friendly plain text: a title and a short bullet list. */
export function buildFileReport(report: FileReport): string {
  const lines = [
    REPORT_TITLE,
    GAME_LINE,
    `- SavePrism version: ${report.version}`,
    `- File extension: ${report.extension === "" ? "(none)" : `.${report.extension}`}`,
    `- Size: ${groupDigits(report.byteSize)} bytes`,
    `- First bytes (hex): \`${report.headHex === "" ? "(none)" : report.headHex}\``,
    `- First bytes (text): \`${report.headAscii === "" ? "(none)" : report.headAscii}\``,
    `- Looks like: ${ENTROPY_TEXT[report.entropy]}`,
    `- SHA-256: ${report.sha256 === null ? "(not computed)" : `\`${report.sha256}\``}`,
  ];
  return lines.join("\n");
}
