// Writes target/release/bundle/nsis/latest.json, the file the in-app updater reads from
// https://github.com/huseyincancalti/saveprism/releases/latest/download/latest.json.
// Usage: node scripts/release/latest-json.mjs <repo root>   (build.sh runs it after the signed build)
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const REPO = "huseyincancalti/saveprism";

const root = process.argv[2] ?? process.cwd();
const config = JSON.parse(readFileSync(join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
const version = config.version;
const setup = `${config.productName}_${version}_x64-setup.exe`;
const dir = join(root, "target/release/bundle/nsis");
const sigPath = join(dir, `${setup}.sig`);
if (!existsSync(sigPath)) {
  console.error(`missing ${sigPath}: build with the updater key (bash scripts/release/build.sh)`);
  process.exit(1);
}

/** The lines under `## <version>` in CHANGELOG.md, up to the next `## ` heading. */
function notesFor(source, wanted) {
  const lines = source.split(/\r?\n/);
  const start = lines.findIndex((line) => {
    const heading = line.startsWith("## ") ? line.slice(3).trim() : null;
    return heading !== null && (heading === wanted || heading.startsWith(`${wanted} `));
  });
  if (start < 0) return "";
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => line.startsWith("## "));
  return (end < 0 ? rest : rest.slice(0, end)).join("\n").trim();
}

const notes = notesFor(readFileSync(join(root, "CHANGELOG.md"), "utf8"), version);
if (notes === "") console.warn(`CHANGELOG.md has no "## ${version}" section; notes stay empty`);

const latest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(sigPath, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/v${version}/${setup}`,
    },
  },
};
const out = join(dir, "latest.json");
writeFileSync(out, `${JSON.stringify(latest, null, 2)}\n`);
console.log(`wrote ${out}`);
