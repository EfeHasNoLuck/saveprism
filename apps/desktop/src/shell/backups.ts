/** Pure helpers of the Backups dialog: when a backup was made and how big it is, in the UI language. */

const SECOND = 1000;
const MINUTE = 60 * SECOND;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export interface Age {
  value: number;
  unit: "second" | "minute" | "hour" | "day";
}

/** The age of something made at `createdMs` in its largest whole unit (a time in the future is 0 s). */
export function ageParts(createdMs: number, nowMs: number): Age {
  const age = Math.max(0, nowMs - createdMs);
  if (age < MINUTE) return { value: Math.floor(age / SECOND), unit: "second" };
  if (age < HOUR) return { value: Math.floor(age / MINUTE), unit: "minute" };
  if (age < DAY) return { value: Math.floor(age / HOUR), unit: "hour" };
  return { value: Math.floor(age / DAY), unit: "day" };
}

/** "5 minutes ago", "yesterday", "now" in the UI language. */
export function ageText(createdMs: number, nowMs: number, language: string): string {
  const { value, unit } = ageParts(createdMs, nowMs);
  try {
    return new Intl.RelativeTimeFormat(language, { numeric: "auto" }).format(-value, unit);
  } catch {
    return "";
  }
}

/** "Oct 23, 2026, 2:05:09 PM" in the UI language (seconds too: two saves can be a minute apart). */
export function dateTimeText(ms: number, language: string): string {
  try {
    return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "medium" }).format(ms);
  } catch {
    return new Date(ms).toISOString();
  }
}

/** "512 B", "12.3 KB", "1.2 MB" with the UI language's decimal mark. */
export function sizeText(bytes: number, language: string): string {
  const [value, unit] =
    bytes < 1024 ? [bytes, "B"] : bytes < 1024 * 1024 ? [bytes / 1024, "KB"] : [bytes / 1024 / 1024, "MB"];
  let number: string;
  try {
    number = new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(value);
  } catch {
    number = String(Math.round(value * 10) / 10);
  }
  return `${number} ${unit}`;
}
