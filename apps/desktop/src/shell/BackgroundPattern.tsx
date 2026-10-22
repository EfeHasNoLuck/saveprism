import { useTheme } from "@/theme";
import { useSettings } from "./context";
import { patternOpacity } from "./settings";

/**
 * A continuous lattice of the two-square star: grid lines run unbroken across tiles and a turned
 * square sits in every cell. It is drawn as a CSS mask in the text colour, so it follows the theme.
 */
const TILE = `<svg xmlns='http://www.w3.org/2000/svg' width='48' height='48' viewBox='0 0 48 48' fill='none' stroke='black' stroke-width='1'><path d='M0 12H48M0 36H48M12 0V48M36 0V48'/><path d='M24 7L41 24L24 41L7 24Z'/></svg>`;
const MASK = `url("data:image/svg+xml,${encodeURIComponent(TILE)}")`;

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
        maskImage: MASK,
        WebkitMaskImage: MASK,
      }}
    />
  );
}
