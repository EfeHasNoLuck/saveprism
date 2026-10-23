# Visual checks over CDP

Drive the running desktop app without screen access. Start it with the WebView2 debug port:

```
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 pnpm -C apps/desktop tauri dev
```

- `node cdp.mjs eval "<js>"` / `node cdp.mjs shot out.png`: evaluate in the page, take a screenshot.
- `node cdp2.mjs …`: also `size W H`, `clearsize`, `hover X Y`, `click X Y`, `key Name`, `type "text"`, `full out.png`.
- `node cdp6.mjs file.js`: evaluate a file with focus emulation (typing works while the window is in the background).
- `patch.js`: paste into the page (via `eval`) to stub the native Open/Save dialogs through `window.fetch`
  (`window.__nextOpen` / `window.__nextSave`); it refuses save targets outside a `scratchpad` folder.
  `window.__TAURI_INTERNALS__.invoke` is read-only, which is why `fetch` is patched.

Never save over files under `fixtures/`.
