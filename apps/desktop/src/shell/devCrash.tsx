/**
 * Development only: open the app with `?crash` in the address (for example
 * `http://localhost:1420/?crash`) to see the crash screen. `main.tsx` mounts this behind
 * `import.meta.env.DEV`, so a production build neither includes nor runs it. The message carries a
 * made-up path so a copied report shows the path stripping at work.
 */
export function DevCrashTrigger() {
  if (new URLSearchParams(window.location.search).has("crash")) {
    throw new Error(String.raw`Test crash from ?crash while reading C:\Users\Example Name\Saves\slot1.sav`);
  }
  return null;
}
