import i18next from "i18next";
import en from "../../locales/en/editor.json";
import tr from "../../locales/tr/editor.json";

export const NS = "editor";

let registered = false;

/**
 * Registers the `editor` namespace on the app's i18n instance. Call it at render time (after
 * `initI18n()` ran in main.tsx), not at import time. Safe to call repeatedly.
 */
export function registerEditorBundles(): void {
  if (registered || !i18next.isInitialized) return;
  registered = true;
  i18next.addResourceBundle("en", NS, en, true, true);
  i18next.addResourceBundle("tr", NS, tr, true, true);
}
