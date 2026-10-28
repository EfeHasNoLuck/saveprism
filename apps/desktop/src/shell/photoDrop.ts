/**
 * Pure routing for a file the window receives from Tauri's drag-and-drop events (HTML5 drops do
 * not carry file paths here). The host reports positions in physical pixels; the DOM hit test
 * needs CSS pixels. Keep this module free of Tauri, React and the DOM so it can be tested.
 */

/** The attribute the Photo card's preview box carries. */
export const PHOTO_DROP_ATTRIBUTE = "data-photo-drop";

/** A position in pixels. */
export interface Point {
  x: number;
  y: number;
}

/** The little bit of an element the hit test needs. */
export interface DropHit {
  closest(selector: string): unknown;
}

export type DropRoute = "ignore" | "open-save" | "photo";

/** Converts a Tauri drop position (physical pixels) to CSS pixels for `elementFromPoint`. */
export function cssPoint(position: Point, devicePixelRatio: number): Point {
  const ratio =
    Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

/** Whether the element at a physical drop position sits inside the photo drop zone. */
export function isOverPhotoDrop(
  position: Point,
  devicePixelRatio: number,
  elementAt: (x: number, y: number) => DropHit | null,
): boolean {
  const point = cssPoint(position, devicePixelRatio);
  return elementAt(point.x, point.y)?.closest(`[${PHOTO_DROP_ATTRIBUTE}]`) != null;
}

/**
 * Decides what a dropped file means. While Settings is open a drop never opens a save: over the
 * photo zone it becomes the background photo, anywhere else it is ignored quietly. With Settings
 * closed it opens the save as before.
 */
export function routeDrop(options: {
  path: string | null | undefined;
  settingsOpen: boolean;
  overPhotoZone: boolean;
}): DropRoute {
  if (!options.path) return "ignore";
  if (!options.settingsOpen) return "open-save";
  return options.overPhotoZone ? "photo" : "ignore";
}
