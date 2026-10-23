<!-- Research by agy (gemini-3.8-flash) on 2026-10-23 from public docs and open-source engine code (behavior only, clean room). Not yet checked line by line; items marked unconfirmed need a source before use. -->

# Save formats: SugarCube, TyranoScript, Ren'Py

> **Correction (2026-10-23, from reading the SugarCube 2 source for behavior: `src/lib/diff.js` at develop and v2.36.1,
> `src/save.js`, `src/state.js`, `src/lib/serial.js`).** The SugarCube details below are partly wrong. What the
> implementation in `apps/desktop/src-tauri/src/session/sugarcube.rs` follows:
> - `delta[i]` is the diff of moment i-1 to moment i (keys sorted), or `null` when nothing changed.
> - In a diff: `0` deletes a key; `[2, v]` is a copy (a changed scalar, a new key, a type change, or a replaced
>   Map/Set/Date/class), so a scalar changed during play is stored as `[2, v]`, never bare; `[1, from, to]` under a `~`
>   key removes array items; `[3, ms]` is a legacy Date copy; a plain object is a nested diff.
> - Revive tuples have two elements, `[tag, payload]`, with tags `(revive:map|set|date|eval)` or `(revive:)`.
> - SugarCube 2.37 with `Config.history.disableDeltas` stores `state.history` instead of `state.delta` (not handled yet;
>   such saves open as the plain Values list).

## 1. Twine SugarCube 2

### Exported Single Save File (`.save`) & Encoding
- **File Format & Bytes:** Exported single save files (`.save`) are UTF-8 plain text files containing an ASCII Base64 string produced by `LZString.compressToBase64()` applied to the UTF-8 serialized JSON of the Save Object.
- **Difference Between 2.36.x and 2.37.x:**
  - *SugarCube 2.36.x:* Export is triggered via `Save.export([filename])`, creating a download Blob containing the LZString Base64 string of the single save.
  - *SugarCube 2.37.x:* The Save API was rewritten and modularized. Single-save disk export was renamed to `Save.disk.save([filename])`. `Save.disk.export()` was repurposed exclusively to export all saves into a bundle. The underlying `lz-string` library was upgraded to v1.5.0 in 2.37.0.
  - *(Sources: [SugarCube 2 Documentation - Save API](https://www.motoslave.net/sugarcube/2/docs/#save-api), [SugarCube 2 Releases](https://github.com/tmedwards/sugarcube-2/releases/tag/v2.37.0))*.

### Save Object Fields
The uncompressed JSON Save Object contains:
- `id`: String matching `Config.saves.id` (validates that the save belongs to this story).
- `title`: String description of the save (defaults to current passage title or configured title).
- `date`: Numeric integer timestamp (Unix epoch in milliseconds, `Date.now()`).
- `version`: Story version string or integer defined via `Config.saves.version` (optional).
- `metadata`: Arbitrary user/author key-value object added via `Save.onSave` handlers (optional).
- `state`: The serialized game state container object.
- `idx`: Present in slot storage metadata mapping to the numeric slot index [unconfirmed for standalone `.save` files].
- *(Source: [SugarCube 2 Documentation - Save API Object Definition](https://www.motoslave.net/sugarcube/2/docs/#save-api))*.

### Story History Storage (`state`), Deltas, and Editing Current Variables
- **Structure of `state`:**
  - `index`: Integer pointer indicating the active moment in the history stack.
  - `history` vs `delta`: When delta-encoding is active, history moments are stored in `state.delta` (an array).
  - `delta` layout: `delta[0]` is a complete moment snapshot containing `{ title, variables }`. Subsequent moments `delta[i]` (`i >= 1`) store patch differentials calculated via structural diffing between consecutive moments (`moment[i-1]` and `moment[i]`).
  - `expired`: Array of passage names pruned from history due to `Config.history.maxStates`.
  - `seed` & `pull`: PRNG determinism state; `seed` holds the base seed string/value, and `pull` tracks the call counter.
- **Accessing and Editing Current Variables:**
  - The current moment variables are at index `state.index`. If `delta.length === 1`, current variables reside directly in `delta[0].variables`.
  - If `delta.length > 1`, reconstructing current variables requires sequentially applying diff patches up to `state.index`.
  - *Editor requirement:* An editor modifying variables cannot merely edit `delta[0]` when `delta.length > 1`, as subsequent delta diffs would overwrite modified keys. The safest approach is either:
    1. Truncate `delta` to a single moment (`delta = [decodedCurrentMoment]`) and set `index = 0`.
    2. Recalculate and re-encode the forward diff patches for subsequent moments.
  - *(Sources: [SugarCube 2 Source `src/state.js`](https://github.com/tmedwards/sugarcube-2/blob/master/src/state.js), [SugarCube 2 State History Docs](https://www.motoslave.net/sugarcube/2/docs/#state-api))*.

### Special Non-JSON Values Revival (`(revive:eval)`)
- **Format:** SugarCube handles non-JSON native types and classes via revival wrappers: `["(revive:eval)", codeString, payloadData]`.
  - `Map`: `["(revive:eval)", "new Map($ReviveData$)", Array.from(entries)]`
  - `Set`: `["(revive:eval)", "new Set($ReviveData$)", Array.from(values)]`
  - `Date`: `["(revive:eval)", "new Date($ReviveData$)", isoTimestampString]`
  - `undefined`: Primitive revival tag representing `undefined`.
  - Custom classes: `JSON.reviveWrapper(code, data)` where `$ReviveData$` is substituted on load.
- **Safe Editor Treatment:** An external editor must **never** execute `codeString` with JavaScript `eval()` or dynamic function execution (avoids arbitrary code execution vulnerabilities). The editor should treat revival objects strictly as passive tagged data tuples, editing only `payloadData` where needed.
- *(Source: [SugarCube 2 toJSON & JSON.reviveWrapper Specification](https://www.motoslave.net/sugarcube/2/docs/#methods-json-method-revivewrapper))*.

### `.savesbundle` Format (SugarCube 2.37+)
- Produced by `Save.disk.export([filename])`.
- It is an LZString Base64-compressed JSON container containing all browser save slots (manual slots plus autosave entries).
- *(Source: [SugarCube 2 Documentation - Save.disk](https://www.motoslave.net/sugarcube/2/docs/#save-api-prototype-method-disk-export))*.

### Save Codes
- In 2.37+, SugarCube provides `Save.base64.save()` and `Save.base64.export()` to generate text strings for copy-pasting.
- A single save code produced by `Save.base64.save()` is identical in content to the ASCII text written into a `.save` file.
- *(Source: [SugarCube 2 Documentation - Save.base64](https://www.motoslave.net/sugarcube/2/docs/#save-api-prototype-method-base64-save))*.

---

## 2. TyranoScript / TyranoBuilder

### Save Locations & Storage
- **Desktop Builds (PC / NW.js / Electron):** Saves are written to the local disk in a `savedata/` folder relative to the game executable. Individual save slots are stored as `.sav` files (commonly `data.sav`, `system.sav`, or prefixed with the project ID from `Config.tjs`).
- **Browser / Web Builds:** When `configSave = "webstorage"` (default in `Config.tjs`), saves are written to browser `localStorage` under keys derived from project configuration (e.g., `<projectID>_tyrano_data` for game slots, `<projectID>_tyrano_sf` for system data).
- *(Sources: [TyranoScript Config.tjs Spec](https://github.com/ShikemokuMK/tyranoscript/blob/master/data/system/Config.tjs), [TyranoScript Official Manual](https://tyranoscript.com/))*

### Encoding Format
- Text files and storage entries contain stringified JSON that is **URL-escaped (percent-encoded)** using standard `encodeURIComponent` / `escape()` formatting (characters such as `"`, `{`, `}`, and `@` appear as `%22`, `%7B`, `%7D`, `%40`).
- Reversible via standard URL percent-decoding (`decodeURIComponent` in JS, `urllib.parse.unquote` in Python).
- When `configSave = "webstorage_compress"` is configured, compression is applied before writing to WebStorage [compression algorithm unconfirmed in public core docs; LZString variant used].
- *(Source: [TyranoScript Core `kag.menu.js`](https://github.com/ShikemokuMK/tyranoscript/blob/master/tyrano/plugins/kag/kag.menu.js))*.

### JSON Field Layout: `f` vs `sf`
- **Game Variables (`f.`):**
  - Scoped to individual save slots.
  - Located within the save slot object under `stat.f` (i.e. `saveData.data[slotIndex].stat.f`). Overwritten when an individual save slot is loaded.
- **System Variables (`sf.`):**
  - Scoped globally across the entire game installation (CG gallery flags, endings unlocked, global settings).
  - Stored separately from game slots: in desktop builds inside `system.sav`, and in browser storage under the system variables key (`_tyrano_sf`). Never overwritten by loading regular save slots.
- *(Source: [TyranoScript Variable Scope Reference](https://tyranoscript.com/))*

---

## 3. Ren'Py

### File Layout of `.save`
- Ren'Py save files are standard **ZIP archives** containing the following members:
  - `log`: Primary binary file containing the serialized game rollback history and variable states.
  - `json`: Plain UTF-8 JSON text containing save metadata (playtime, Ren'Py engine version, game version, save name, timestamp).
  - `screenshot.png`: PNG image thumbnail of the game viewport captured at save creation.
  - `extra_info`: Plain text UTF-8 file containing the string passed to `renpy.save` (e.g. passage or chapter title).
  - `renpy_version`: Plain text file recording the engine version string.
  - `signatures`: Plain text file containing cryptographic signature lines (present in 7.6+/8.1+).
- *(Sources: [Ren'Py Documentation - Save Functions and File Formats](https://www.renpy.org/doc/html/save_dump.html), [Ren'Py Source `renpy/loadsave.py`](https://github.com/renpy/renpy/blob/master/renpy/loadsave.py))*.

### Game State Serialization
- The `log` member holds the entire game state.
- It is serialized using Python's native **`pickle`** module, written with the highest supported pickle protocol (`pickle.HIGHEST_PROTOCOL`, Protocol 4 in Python 3 / Ren'Py 8, Protocol 2 in Python 2 / Ren'Py 7).
- *(Source: [Ren'Py Source `renpy/loadsave.py`](https://github.com/renpy/renpy/blob/master/renpy/loadsave.py))*.

### Signing Scheme (Ren'Py 7.6 / 8.1+)
- **Keys Location:** Private/public signing keys are generated per player and stored in the user profile directory:
  - Windows: `%APPDATA%\RenPy\tokens\`
  - macOS: `~/Library/RenPy/tokens/`
  - Linux: `~/.renpy/tokens/`
  - Key file: `security_keys.txt` (storing ECDSA private/public key records).
- **What is Signed:** The digital signature is computed strictly over the raw binary bytes of the **`log`** archive member using **ECDSA**.
- **Signatures File Layout:** Contains lines with the base64-encoded DER public key followed by the base64-encoded signature.
- **Save Editor Compatibility & Player Prompts:**
  - If a save editor modifies `log`, the existing signature is invalidated.
  - To prevent warnings, an editor must load the player's private key from `security_keys.txt`, re-sign the new `log` bytes, and update `signatures`.
  - If no matching key or signature exists, Ren'Py displays a confirmation prompt: *"This save was created on a different device. Maliciously constructed save files can harm your computer. Do you trust the device this save was created on and want to load it?"*
  - The player **can simply accept this prompt** (select "Yes") to proceed with loading the edited save normally.
- *(Sources: [Ren'Py Source `renpy/savetoken.py`](https://github.com/renpy/renpy/blob/master/renpy/savetoken.py), [Ren'Py Changelog 8.1.0](https://www.renpy.org/doc/html/changelog.html))*.
