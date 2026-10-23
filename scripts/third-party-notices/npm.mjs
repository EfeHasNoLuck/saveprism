// Prints the "Frontend packages" part of THIRD_PARTY_NOTICES.md.
// Usage: node npm.mjs <vite outDir built with --sourcemap> <apps/desktop dir>
// The bundled packages are read from the JS source maps (exactly what Vite shipped), plus the packages that
// src/index.css imports (Tailwind, animations, fonts), which do not show up in JS source maps.
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

const [outDir, appDir] = process.argv.slice(2);
if (!outDir || !appDir) {
  console.error("usage: node npm.mjs <vite outDir> <apps/desktop dir>");
  process.exit(2);
}

/** Folder of the package that contains `file`: the nearest parent below node_modules with a package.json. */
function packageDir(file) {
  let dir = dirname(file);
  while (dir.includes("node_modules")) {
    const manifest = join(dir, "package.json");
    if (existsSync(manifest)) {
      const pkg = JSON.parse(readFileSync(manifest, "utf8"));
      // Some packages ship nested package.json files without a name ({"type":"module"}).
      if (pkg.name && pkg.version) return dir;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function mapFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return mapFiles(path);
    return name.endsWith(".map") ? [path] : [];
  });
}

const dirs = new Set();
for (const map of mapFiles(outDir)) {
  const { sources = [], sourceRoot = "" } = JSON.parse(readFileSync(map, "utf8"));
  for (const source of sources) {
    if (!source.includes("node_modules")) continue;
    const dir = packageDir(resolve(dirname(map), sourceRoot, source));
    if (dir) dirs.add(dir);
  }
}

// Packages imported from CSS: `@import "pkg";` or `@import "pkg/file.css";`.
const css = readFileSync(join(appDir, "src", "index.css"), "utf8");
for (const [, spec] of css.matchAll(/@import\s+"([^".][^"]*)"/g)) {
  const parts = spec.split("/");
  const name = spec.startsWith("@") ? parts.slice(0, 2).join("/") : parts[0];
  const manifest = join(appDir, "node_modules", name, "package.json");
  if (existsSync(manifest)) dirs.add(dirname(manifest));
}

const LICENSE_FILE = /^(licen[cs]e|copying)(\.(md|txt))?$/i;
const packages = [...dirs]
  .map((dir) => {
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const file = readdirSync(dir).find((name) => LICENSE_FILE.test(name));
    const repo = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url;
    return {
      name: pkg.name,
      version: pkg.version,
      license: typeof pkg.license === "string" ? pkg.license : (pkg.license?.type ?? "UNKNOWN"),
      repository: repo?.replace(/^git\+/, "").replace(/\.git$/, ""),
      text: file ? readFileSync(join(dir, file), "utf8").trim() : null,
    };
  })
  // The same package can be reached through two paths; keep one entry per name@version.
  .filter((p, i, all) => all.findIndex((q) => q.name === p.name && q.version === p.version) === i)
  .sort((a, b) => a.name.localeCompare(b.name));

const counts = {};
for (const p of packages) counts[p.license] = (counts[p.license] ?? 0) + 1;

const out = [];
out.push("## Frontend packages", "");
out.push("The app window bundles these npm packages. Licenses used:", "");
for (const [license, count] of Object.entries(counts).sort()) out.push(`- ${license}: ${count} package(s)`);
out.push("");
for (const p of packages) {
  out.push(`### ${p.name} ${p.version}`, "");
  out.push(`License: ${p.license}${p.repository ? ` · ${p.repository}` : ""}`, "");
  if (p.text) out.push("```text", p.text, "```", "");
  else out.push(`No license file is shipped in the package; see the ${p.license} license text.`, "");
}
process.stdout.write(out.join("\n") + "\n");
console.error(`npm.mjs: ${packages.length} packages, licenses: ${Object.keys(counts).sort().join(", ")}`);
