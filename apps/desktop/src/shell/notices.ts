/**
 * The third-party notices as data. The text is imported at build time with Vite's `?raw` inside a
 * dynamic import, so it becomes a lazy chunk instead of part of the startup bundle.
 */

/** One package from `THIRD_PARTY_NOTICES.md`. */
export interface NoticePackage {
  name: string;
  version: string;
  license: string;
  url: string;
  /** The license text of the package, or an empty string when the file has none. */
  text: string;
  source: "rust" | "frontend";
}

const USED_BY = /^-\s+(.+?)\s+(\S+)\s+\((\S+)\)\s*$/;
const FRONTEND_HEADING = /^(.+?)\s+(\S+)$/;
const FENCE = "```";

/**
 * Parses the two shapes of the notices file:
 * - `## Rust crates`: `### <license>` groups with `Used by:` lines and one shared `text` block.
 * - `## Frontend packages`: `### <name> <version>`, `License: <id> · <url>` and a `text` block.
 */
export function parseNotices(source: string): NoticePackage[] {
  const packages: NoticePackage[] = [];
  const lines = source.split(/\r?\n/);
  let region: NoticePackage["source"] | null = null;
  let license = "";
  let usedBy: { name: string; version: string; url: string }[] = [];
  let frontend: { name: string; version: string; license: string; url: string } | null = null;
  let text: string | null = null;
  let fence: string[] | null = null;

  const flush = () => {
    if (region === "rust" && license !== "") {
      for (const entry of usedBy) {
        packages.push({ ...entry, license, text: text ?? "", source: "rust" });
      }
    } else if (region === "frontend" && frontend !== null) {
      packages.push({ ...frontend, text: text ?? "", source: "frontend" });
    }
    usedBy = [];
    frontend = null;
    text = null;
  };

  for (const line of lines) {
    if (fence !== null) {
      if (line.trim() === FENCE) {
        text = fence.join("\n");
        fence = null;
      } else {
        fence.push(line);
      }
      continue;
    }
    if (line.trim() === "```text" || line.trim() === FENCE) {
      fence = [];
      continue;
    }
    if (line.startsWith("## ")) {
      flush();
      const title = line.slice(3).trim();
      region = title === "Rust crates" ? "rust" : title === "Frontend packages" ? "frontend" : null;
      license = "";
      continue;
    }
    if (line.startsWith("### ")) {
      flush();
      license = "";
      const heading = line.slice(4).trim();
      if (region === "rust") {
        license = heading;
      } else if (region === "frontend") {
        const match = FRONTEND_HEADING.exec(heading);
        if (match) {
          frontend = { name: match[1], version: match[2], license: "", url: "" };
        }
      }
      continue;
    }
    if (region === "rust" && line.startsWith("- ")) {
      const match = USED_BY.exec(line);
      if (match) {
        usedBy.push({ name: match[1], version: match[2], url: match[3] });
      }
      continue;
    }
    if (region === "frontend" && frontend !== null && line.startsWith("License: ")) {
      const value = line.slice("License: ".length);
      const [id, url] = value.split(" · ");
      frontend.license = (id ?? "").trim();
      frontend.url = (url ?? "").trim();
    }
  }
  flush();
  return packages;
}

let cached: Promise<NoticePackage[]> | null = null;

/** Loads and caches the parsed notices (the dynamic import keeps them out of the startup bundle). */
export function loadNotices(): Promise<NoticePackage[]> {
  cached ??= import("../../../../THIRD_PARTY_NOTICES.md?raw").then((module) =>
    parseNotices(module.default),
  );
  return cached;
}
