/*
 * Applies the stored theme before React runs, so the opening cover uses the same background as
 * the app. Classic script (the CSP allows files from the app, not inline scripts). The rules are
 * the same as src/theme/theme.ts: only "light" and "dark" are preferences, everything else
 * follows the OS.
 */
(function () {
  try {
    const stored = window.localStorage.getItem("saveprism.theme");
    const dark =
      stored === "dark" ||
      (stored !== "light" &&
        window.matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.style.colorScheme = dark ? "dark" : "light";
  } catch {
    // Storage or matchMedia is unavailable: the light cover is used for this start.
  }
})();
