/** Pure helpers that turn host facts into display text. */

const EXTENSION_LABELS: Record<string, string> = {
  rmmzsave: "RPG Maker MZ",
  rpgsave: "RPG Maker MV",
  rvdata2: "RPG Maker VX Ace",
  rvdata: "RPG Maker VX",
  rxdata: "RPG Maker XP",
  es3: "Easy Save 3",
  rsv: "RAGS",
  sav: "SAV",
};

const CODEC_LABELS: Record<string, string> = {
  "format.json": "JSON",
  "format.rpgmaker-json": "RPG Maker",
  "format.es3": "Easy Save 3",
  "format.es3-json": "Easy Save 3",
  "format.gvas": "Unreal",
  "format.ruby-marshal": "Ruby Marshal",
  "format.nrbf": ".NET BinaryFormatter",
};

const ENGINE_LABELS: Record<string, string> = {
  sugarcube: "SugarCube",
  "rpgmaker-mv": "RPG Maker MV",
  "rpgmaker-mz": "RPG Maker MZ",
};

/**
 * Short engine or format name for the badge; `null` when nothing is known. `engine` is the engine
 * the host found by the document's shape; it wins over the file name and the codec.
 */
export function engineLabel(fileName: string, codec: string, engine?: string | null): string | null {
  const byShape = engine ? ENGINE_LABELS[engine] : undefined;
  if (byShape) return byShape;
  const dot = fileName.lastIndexOf(".");
  const extension = dot >= 0 ? fileName.slice(dot + 1).toLowerCase() : "";
  const byExtension = EXTENSION_LABELS[extension];
  if (byExtension && extension !== "sav") return byExtension;
  return CODEC_LABELS[codec] ?? byExtension ?? null;
}

/** Last part of a path, for either kind of separator. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/**
 * Default file name of "Export .json": `file1.rmmzsave` becomes `file1-export.json`, so it never
 * equals the name of a `.json` save.
 */
export function exportFileName(fileName: string): string {
  const dot = fileName.lastIndexOf(".");
  const stem = dot > 0 ? fileName.slice(0, dot) : fileName;
  return `${stem}-export.json`;
}

/** Folder part of a path (empty when there is none). */
export function dirName(path: string): string {
  const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return index >= 0 ? path.slice(0, index) : "";
}
