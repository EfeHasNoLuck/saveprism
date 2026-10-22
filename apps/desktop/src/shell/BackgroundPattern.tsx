import { useTheme } from "@/theme";
import { useSettings } from "./context";
import { patternOpacity } from "./settings";

/**
 * A continuous lattice of the logo's two-square star. Every 64 px tile holds one star (a turned
 * square and an upright square of the same size) and eight spokes: the turned square's points run
 * on to the tile edges and the upright square's corners to the tile corners, where they meet the
 * spokes of the next tiles. So every star is joined to its eight neighbours and the lines run
 * unbroken across the window. Both layers are SVG masks painted in the two brand blues
 * (`--pattern-square`, `--pattern-diamond`), so they follow the theme.
 */
const TILE = 64;
const C = TILE / 2;
/** Distance from the centre to a point of the star. */
const R = 18;
/** Half the side of the upright square (its corners are R from the centre too). */
const H = R / Math.SQRT2;

const n = (value: number) => Number(value.toFixed(2));
const [LOW, HIGH] = [n(C - H), n(C + H)];

function mask(paths: string): string {
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${TILE}' height='${TILE}' viewBox='0 0 ${TILE} ${TILE}' fill='none' stroke='black' stroke-width='1'>${paths}</svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

/** Upright square and its diagonal spokes. */
const SQUARE = mask(
  `<path d='M${LOW} ${LOW}H${HIGH}V${HIGH}H${LOW}Z'/>` +
    `<path d='M${LOW} ${LOW}L0 0M${HIGH} ${LOW}L${TILE} 0M${HIGH} ${HIGH}L${TILE} ${TILE}M${LOW} ${HIGH}L0 ${TILE}'/>`,
);
/** Turned square and its straight spokes. */
const DIAMOND = mask(
  `<path d='M${C} ${C - R}L${C + R} ${C}L${C} ${C + R}L${C - R} ${C}Z'/>` +
    `<path d='M${C} ${C - R}V0M${C + R} ${C}H${TILE}M${C} ${C + R}V${TILE}M${C - R} ${C}H0'/>`,
);

export function BackgroundPattern() {
  const { settings } = useSettings();
  const { resolvedTheme } = useTheme();
  if (!settings.patternOn) return null;
  const opacity = patternOpacity(settings.patternStrength, resolvedTheme === "dark");
  return (
    <div
      aria-hidden="true"
      className="app-pattern"
      style={{
        ["--pattern-opacity" as string]: String(opacity),
        ["--pattern-square-mask" as string]: SQUARE,
        ["--pattern-diamond-mask" as string]: DIAMOND,
        ["--pattern-tile" as string]: `${TILE}px`,
      }}
    />
  );
}
