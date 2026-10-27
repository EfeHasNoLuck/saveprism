# Adding a theme

SavePrism has one set of CSS variables with a light and a dark version. A theme is a block of
values for those variables; there is no separate stylesheet per theme.

## Where the variables live

`apps/desktop/src/index.css`:

- `:root` holds the light values and `color-scheme: light dark`.
- `.dark` holds the dark values. The theme code puts this class on `<html>`.
- `@theme inline` maps the variables to Tailwind utilities (`bg-card`, `text-muted-foreground`,
  `border-border`, …), so components use class names, not raw colours.

The groups:

- Surfaces: `--background` (canvas), `--chrome` (header and bars), `--card` (panels), `--popover`,
  `--field` and the `--surface-*` steps for hover, chips and tags.
- Text: `--foreground`, `--card-foreground`, `--muted-foreground` and the `*-foreground` pairs.
- Actions: `--primary*`, `--secondary*`, `--accent*`; `--tertiary*` is the cyan accent (Gold).
- State: `--changed*` marks values that differ from the file, `--destructive` and `--error-*`
  mark values that cannot be saved, `--success` and `--warning` are the remaining hints.
- Structure: `--border`, `--input`, `--ring`, `--radius` and the `--pattern-*` colours of the
  background lattice.

## Choosing and storing a theme

- `apps/desktop/src/theme/theme.ts` is the pure part: `ThemePreference` (`light`, `dark`,
  `system`), `parseStoredPreference` and `resolveTheme` (follows the OS for `system`).
- `apps/desktop/src/theme/index.tsx` reads and writes the preference under the localStorage key
  `saveprism.theme`. `applyStoredTheme()` runs in `main.tsx` before the first render (and
  `public/boot.js` applies the same choice to the opening cover); the provider keeps `<html>` in
  sync: it toggles `.dark` and sets `style.colorScheme`.
- The theme is chosen in Settings → General, which offers `system`, `light` and `dark` with
  `useTheme()`. There is no theme switch in the header.

## Colour presets

`apps/desktop/src/shell/colors.ts` holds the four presets for each group (`changed`, `accent`,
`error`). `colorCss(choices)` writes only the variables of a non-default choice, in both theme
blocks; the defaults write nothing and leave `index.css` as it is. `shell/appearance.ts`
`applyColors` puts that CSS into a `<style id="saveprism-colors">` element, at startup and on
every settings change.

## Theme files

Settings → Appearance can export the current look as a file and import one made by someone else
("Community themes" links to the GitHub Discussions). A theme file is data only — no code, no
image — and is validated before anything changes.

- `shell/themeFile.ts` holds the schema, `buildThemeFile`, `parseThemeFile` and
  `themeSettingsPatch`; `shell/themeFile.test.ts` covers every refusal.
- `session/theme_file.rs` writes and reads the file with the same checks as "Export .json": never
  over the open save, never a folder, a name ending in `.saveprism-theme.json`, a temporary file
  renamed into place, and at most 64 KB.
- Import applies a photo theme to the photo already in the app folder; with no photo it falls back
  to the Default pattern (the image itself is not part of the file in v0.1).

```json
{
  "format": "saveprism-theme",
  "version": 1,
  "name": "Blue night",
  "colors": { "changed": "violet", "accent": "#123abc", "error": "red" },
  "textSize": 110,
  "background": { "kind": "pattern", "pattern": "dots", "strength": 70 }
}
```

A photo background uses
`{ "kind": "photo", "fit": "fit" | "fill", "blur": 0..100, "darken": 0..100 }`. Colors are a
preset id of their group (`violet`, `rose`, `cyan`, …) or a `#rrggbb` value; `textSize` is an
integer percent from 80 to 140. Unknown keys are ignored; a wrong type, an unknown preset or a
value outside its range refuses the whole file with a short reason.

## Adding a high-contrast theme

1. `index.css`: add a block such as `.contrast { … }` with the stronger values. Decide how it
   combines with dark (for example `.dark.contrast`) and keep every background/text pair legible.
2. `theme/theme.ts`: add `"contrast"` to `ThemePreference` and accept it in
   `parseStoredPreference`; extend `resolveTheme` if the theme needs both a light and a dark mode.
3. `theme/index.tsx`: toggle the new class in `applyToDocument` (remove it when another theme is
   chosen).
4. `shell/SettingsPage.tsx`: add it to `THEMES`, and add `theme.contrast` to
   `locales/en/common.json` and `locales/tr/common.json`.
5. Contrast: keep text at 4.5:1 or more on every surface and row tint it can sit on.
   `shell/colors.test.ts` computes WCAG luminance and contrast for each preset in both themes:
   preset text on the card, container text on its container, and the muted label and main text on
   the tinted changed/error rows. If the new theme also has colour presets, update `CARD` and
   `MUTED` there with its surface colours and run `pnpm test`. The plain theme values are checked
   by hand (or with a browser contrast tool), because the test only covers presets.
