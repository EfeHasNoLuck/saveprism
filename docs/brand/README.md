# SavePrism brand files

Made by the maintainer (2026-10-25). SVG files are true vectors; PNG files are fixed renders of them.

| File | Use |
|---|---|
| `SavePrism-logo-on-dark` | Horizontal logo, light text, transparent background (dark pages) |
| `SavePrism-logo-on-light` | Horizontal logo, navy text, transparent background (light pages) |
| `SavePrism-logo-navy` | Horizontal logo on the navy background |
| `SavePrism-icon-navy` | App icon on the navy square; source of `apps/desktop/src-tauri/icons/` |
| `SavePrism-icon-transparent` | App icon without background |
| `SavePrism-mono-light`, `SavePrism-mono-dark` | One-color icon |
| `SavePrism-icon-small` | Simplified icon for very small sizes |
| `SavePrism-icon-outline` | Outline icon |
| `SavePrism-favicon-*.png`, `SavePrism-favicon.ico` | 16-64 px PNGs and a 256 px ICO |
| `SavePrism-overview` | The whole family on one sheet |

Colors: navy `#031427`, light blue `#ABC7FF`, blue `#5D90E8`, cyan `#4CD7F6`.

The text in the logo SVGs is live text in Segoe UI (falls back to Arial or sans-serif), so it can look different on a
PC without Segoe UI; use the PNGs where that matters.

Regenerate the app icons: `pnpm -C apps/desktop tauri icon <repo>/docs/brand/SavePrism-icon-navy.svg -o <scratch dir>`,
then copy only the files that already exist in `apps/desktop/src-tauri/icons/`.
