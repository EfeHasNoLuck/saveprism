<!-- Clean-room behavior map of saveeditonline.com. Analyst session, 2026-10-24. Plain-language facts about BEHAVIOR only:
     no code, no regexes, no offset/id tables, no keys or key lists (AGENTS.md rule 6). -->

# saveeditonline.com: behavior map (clean room)

Date: 2026-10-24. Purpose: know exactly what the competitor does, so SavePrism can do all of it and better.
Everything here is written in my own words. Nothing was copied from the site's code.

## 0. Method, limits and what was NOT done

**Terms of Service.** The site has none that I could find. `/Terms` and `/Privacy` return 404, the sitemap lists only the home page
and `/formats`, and no page has a legal or privacy section. The only legal-type text is a footer sentence saying that everything
submitted or uploaded is deleted after one day, plus a copyright line. So no ToS limited this analysis. I still kept to the
clean-room rules: public page text, the JavaScript the site serves to every visitor, and the network traffic a normal user's browser
produces. `robots.txt` allows crawling except `/Error/` and `/Contact`. One disclosure: a single read of the public `/Contact` page
happened (through a page-fetch tool) before I had read `robots.txt`. It was not repeated and the contact form was never submitted.

**Respect for the site.** One load per page (home, `/formats`), no crawling, no load test, no login or Patreon flow, nothing
behind a paywall. All of the site's parsing runs on its server, so there is no WASM and no client-side parser to read.

**Uploads.** The brief allowed 5 files from `fixtures/synthetic/` only. I made 4 successful uploads and the 5th was refused by the
site's own free-tier limit (section 4.5); I did not retry or work around it. Files used (all invented data from this repo):

| # | File (under `fixtures/synthetic/`) | Why |
|---|---|---|
| U1 | `renpy/renpy8-1-signed/1-1-LT1.save` | Ren'Py editor and the re-signing behavior |
| U2 | `rpgmaker-mv/file1.rpgsave` | RPG Maker editor, names, "full data" view |
| U3 | `sugarcube/one-moment.save` | `.save` extension shared with Ren'Py: does detection use content? |
| U4 | `godot/godot3.sav` | `.sav` extension shared with Unreal/Wolf/QSP: content detection and heuristics |
| U5 | `renpy/persistent-signed/persistent` | Refused before the file was sent (rate limit) |

For U1 I also asked the site to save an edited copy (one number changed, then a second save with another value) and inspected the
returned bytes locally, as data only (opened with a zip reader, never run). A first save request with an empty change list returned an
HTML page instead of a file (the real editor blocks that case in the browser; this happened twice by my own mistake). Four save requests in
total, all on my own U1 upload: two with an empty change list, two real edits.
Nothing from `fixtures/external/` was used.

**Sources used (tags appear in brackets below).**
- [HTML] Home page and `/formats` page markup read directly, one request each.
- [JS] The JavaScript the site serves: the home page's inline script, `support-requests.js`, `rpg-maker-form.js`, and the editor
  script that arrives inside the editor fragment. Read for behavior; no code is reproduced here.
- [WF] Public page text through a page-fetch tool (summaries): `/`, `/formats`, `translate.`, `legacy.`, the Patreon page.
  These are model summaries, so treat details that only come from [WF] as medium confidence.
- [NET] The network log of an isolated headless Chrome (temp profile) while doing the uploads. The log came from a small
  DevTools-protocol script of mine; REA attached to the same browser.
- [REA] `list_browser_targets`, `inspect_web_page` (scripts, storage, resources) and `analyze_web_bundle` (static analysis of the
  page's JavaScript: 9 scripts parsed, 0 failures). Both ran against the page after the uploads.
- [UP] What the editor showed for U1 to U5.
- [REPO] This repo (README, NOW.md, source), for the SavePrism side of the comparison.

## 1. What the site is

A server-side web application (Microsoft IIS with ASP.NET Core [NET]). The front end is plain jQuery 1.12 and Bootstrap 3
with the jQuery File Upload plugin [REA][HTML]. There is no client-side parser or WebAssembly [NET][REA]. The user's file is
uploaded; the server decodes it, turns it into editable values, keeps the original, and rebuilds the file when the user clicks
Download. The site says it is free, supported through Patreon, English only. One upload box handles everything and the
server picks the editor ("detect the format automatically and open the best editor available") [HTML].

Its own sub-products and mirrors: a "Game Translator" (`translate.saveeditonline.com`, a game-text extractor and editor for RPG Maker
VX Ace/MV/MZ, Ren'Py `.rpa/.rpyc`, Wolf and Twine HTML games) [WF]; a "legacy" editor site [WF]; mirror hostnames `s1.` and `s2.`
showed up in a web search result (not visited).

## 2. Formats and detection

### 2.1 What the home page lists (13 cards, newest first) [HTML]
The cards are informational only. They are not buttons; there is one upload box for all of them.
- Java serialization, HTML games and "packed files" (newest card, 2026-04-14): text and numeric variables.
- RAGS Player 2.4 `.rsv` (described as an improved Unity editor).
- Wolf RPG Editor `.sav` (text and numeric variables; the card admits saves carry no variable names, so the user must change a
  value in game and compare two saves).
- RPG Maker 2000/2003 `.lsd`: gold, items, stats.
- Unreal Engine `.sav` (UE 4.x to 5.7, GVAS): string, int, long, float, double, bool. Gives the usual save folder.
- Unity / .NET (`.dat`, `.save`, `.es3`, any): money, items, stats. Gives the usual folder and says saves can also live in the registry.
- Adobe Flash `.sol` (strings, numbers, booleans; says to close the game first; gives the folder).
- The Witcher 3 `.sav`: money, items, and a Gwent card collection that shows which cards are missing.
- RPG Maker XP `.rxdata`; VX `.rvdata` (VX Ace also works); MV `.rpgsave`; VX Ace `.rvdata2`.
- Ren'Py `.save` (scans for variables; warns to edit only values you recognize).
The intro also names Godot and "HTML games", and the supported-file line names `.sqlite`, `.sol`, `.gz`, `.es3`, `.dat`, `.rmmzsave`.

### 2.2 What `/formats` adds (the page home does not list) [HTML]
The formats page is far bigger. I list it so we know the claimed scope; I could confirm only the formats in section 2.3.
- **Engines:** Ren'Py, AliceSoft, LiveMaker, Emuera, GameMaker, Construct Classic, Aoi Game System / SofthouseChara, Unity/.NET,
  Adventure Creator (Unity), Pixel Crushers, Adventure Game Studio (`agssave.*`), Artemis Engine, Pixel Game Maker MV, Smile Game Builder /
  YUKAR (`.sgs`), Unreal 4/5, Godot ("heuristic" support: `.save .dat .json .cfg .ini`, some `.res/.tres`), HTML games, SugarCube,
  all RPG Maker versions (MZ, MV, VX Ace, VX, XP, 2000/2003), QSP, Terraria (`.plr`, `.wld`), The Witcher 3, Flash `.sol`, Wolf RPG,
  RAGS, TyranoBuilder, Naninovel (`.json .nson`), KiriKiri/KAG (`.ksd`), Visionaire Studio, NScripter/ONScripter save v2,
  Ruby Marshal/RGSS (including some `.dat`).
- **Save containers:** BioWare ERF, BND4 (FromSoftware), MPQ, Brickadia BRZ, PS1 and PS2 memory cards, PS2 `.psu`, Snes9x snapshots,
  RetroArch save states, SharkPort GBA.
- **Serialization and generic formats:** Ruby Marshal, Python pickle, .NET BinaryFormatter, Easy Save 2 and 3, Odin Serializer,
  Java serialization, CBOR, MessagePack, Bitser, NBT (Minecraft), BioWare GFF, SSSoftware Serialize, SCS `.sii`, Protocol Buffers,
  JSON, BSON, XML (raw text editing), plist, SQLite, LiteDB, RocksDB, LevelDB, "compressed JSON and packed text containers", and a
  "generic binary analyzer for unencrypted files".
- **Built-in decompression/decoding:** GZip, zlib, raw Deflate, BZip2, Zstd, LZ4, LZF, LZMA-Alone, XZ, LZip, Oodle/Kraken, Brotli,
  Base64 and Base64URL, LZString, ZIP, TAR, "simple XOR-based obfuscation".
- **Text encodings:** UTF-8, UTF-16 and UTF-32 (both byte orders), GB18030, Shift-JIS, CP949, Windows-1251/1252.
- **Extensions listed:** the generic ones (`.sav .save .dat .bin .bak .slot .profile .player .progress .state .gam .sgm`), the RPG Maker
  ones, engine-specific ones (`.es3 .asd .as2 .asdbak .vsf .saves .sgs .sol .rsv .qsp .ksd .nson .res .tres`), text/structured
  (`.json .xml .plist .ini .cfg .conf .txt .yaml .yml .properties .csv .toml .sii`), databases and serialized
  (`.db .sqlite .sqlite3 .db3 .s3db .sl3 .sqlitedb .ldb .log .sst .pkl .pickle .msgpack .cbor .pb .pbf .protobuf .protobin .bytes`),
  compressed (`.zip .gz .bz2 .zst .lz4 .lzma .xz .lz .tar .br .b64`), containers (`.erf .mod .hak .brz .psu .sps .xps`).
- The page says support "varies with game, engine version, serialization method, encoding, and whether the file is compressed or
  encrypted"; the same extension can mean different engines; some `.dat` files are Ruby Marshal; some web/mobile saves have no
  stable extension. Requests for a new game go through the Contact form. An older "legacy" editor is still linked.

### 2.3 What I confirmed by upload [UP]
- Ren'Py `.save` (U1): opened. RPG Maker MV `.rpgsave` (U2): opened in a dedicated form. SugarCube `.save` (U3): opened as a
  SugarCube story. Godot 3 binary `.sav` (U4): opened as a value tree. Four formats from one upload box, shown in two editor shells (the generic
  value list and the RPG Maker form).

### 2.4 How detection behaves
- **By content, not by extension.** U3 has the extension that Ren'Py uses and was still opened as a SugarCube story (labels like
  "state > delta > variables (Ash) > gold"). U4 has an extension shared by Unreal, Wolf, QSP and others and opened as Godot.
  Pasted text is wrapped as a file named `savegame.txt` and must also be detected by content [JS][UP].
- The formats page confirms the idea (a `.dat` can be Ruby Marshal) [HTML]. Not tested: a file with a wrong extension that has valid
  content for a named engine, and what the site says when nothing matches. The failure path exists (section 3.7) but I did not trigger it.
- Detection and decoding happen on the server after upload; the browser only checks size and readability first.

## 3. How editing works

There are two editor shells and a "raw" fallback.

### 3.1 The generic value editor (used for Ren'Py, SugarCube, Godot and many others) [UP][JS]
- The server sends the save as a list of editable leaf values (numbers, booleans, strings) plus a string table of key names.
  The browser shows a **flat list**, not a tree. Each row is a path such as "inventory > gold", with the value on the right.
- Filter box: Numbers (default), Booleans, Strings, ALL. A search box matches paths and values. Ranking: an exact value or exact
  number first, then values that contain the text, then paths that contain it. The list is paged.
- **Labels from data:** list items are labelled with a sibling field called "name" or containing "name" (or a key field), so
  actors and characters show their names ("actors > _data[1] Harold > _level") [UP][JS].
- **Editing:** clicking a number opens the browser's own small "Insert the value" prompt (one value at a time; only plain numbers with a
  dot as decimal mark and an optional exponent are accepted, anything else, including a decimal comma, shows an "invalid number"
  alert). Booleans toggle on click. Strings open a modal. A "repeat last number" arrow next to each number applies the
  last value you typed; after an edit the arrow becomes an "x" that cancels that edit. This is the only bulk-like helper.
- **String rules are set per format by the server:** free text, text that must keep the same length (fixed-size binary strings), or
  strings locked (read-only). One mode also forces single-line values (INI-style files) [JS].
- **Big integers** that do not fit a double are shown as quoted strings (U1: a 70-bit number appears under Strings) [UP].
- **Compare with an earlier upload:** number rows can carry a green "+N" or red "-N" difference against a previous save of the same
  game, and the server payload has a "comparison enabled" flag and a changed-number count. I saw the flag off in my sessions; I could
  not trigger it with one file per game. The RPG Maker form has the same idea (section 3.2) [JS][NET].
- **No** add/remove of entries, **no** undo/redo, **no** tree view, **no** multi-row select, **no** file preview before download [JS].
- Footer: a Download button (disabled until something changed), a "Changes: N" counter, and an "Edit raw JSON (full data)" button [UP].

### 3.2 The RPG Maker form (MV/MZ and also XP/VX/VX Ace/2000/2003 per the site) [UP][JS]
- U2 showed: **Gold**; a **Characters** picker ("Character #1 - Harold") with current HP and MP and eight stat-modifier boxes
  (hints say these modifiers are added to database stats, not final values); **Inventory** (items), **Weapons**, **Armors**,
  **Variables**, each with a search box ("ID, name or value"), a pager (40 per page, "1 / 1 - 3 records"), and per-row value buttons.
- **No Switches section** in the form (switches only appear as booleans in the full-data view).
- **Names:** without the game's data files, entries show only "Item #5". The help text says item names live in `www/data/Items.json` and
  variable names in `www/data/System.json`, and that the user can upload that file to the editor. I did not exercise this.
  The form script can also list names that are not in the save yet (so missing items can be set), which suggests items can be added once names are known.
- **Compare with the previous upload:** the form can mark entries "new" or show +/- changes versus a previous save and sorts changed entries first [JS].
- Number edits use a prompt for list values and text boxes for gold and stats; the same "repeat last number" and "x" helpers exist.
- Saving posts the whole edited sections (or a patch) back to the server; the file comes back with the same name [JS][NET].
- Button "Edit JSON (full data)" opens the generic editor (3.1) on the same file: U2 gave about two pages of rows including party gold,
  actor levels/HP/MP/EXP, system counters, screen/timer values, variables, skills and equipment [UP].

### 3.3 Ren'Py (U1) [UP]
- Shows a green notice on top: the file "will be automatically re-signed with keys unique to each user, so it appears to come from
  a different virtual device".
- Variables appear as flat rows: top-level numbers, dotted names kept as one name ("mystery.clue_count"), nested dict values
  ("inventory > gold", "stats > luck"), list/tuple items ("position[0]"), floats, negatives, and a 70-bit and a 600-digit integer shown as strings.
- Strings tab and Booleans tab present. The home card tells the user to change only values they recognize.
- Not tested: `persistent` (U5 was blocked), saves from Ren'Py 7 or 8.3 with other pickle protocols.

### 3.4 Other formats (from the site's own text only; not exercised)
Wolf: no names, so compare two saves. Unreal: typed values. Flash: strings/numbers/booleans. Witcher 3: money, items, Gwent collection with
missing cards. Unity: money/items/stats, with the registry mentioned as a possible source. HTML games and Java: text and number variables.

### 3.5 Raw JSON ("full data") [UP]
The "raw JSON" button does not give a text box. It opens the same flat editor over every value, via a single feature call on the server
(`ApplyFeature ... CanEditAsRawJson`). No add/remove there either.

### 3.6 Save and download [NET][UP][JS]
- Download is disabled until at least one value changed. A "Preparing download, please wait" hint shows for five seconds.
- The browser first pings a readiness endpoint so that, during a site update, the user gets a "site is being updated, try again in a minute" alert instead of a broken download.
- The generic editor sends only the **list of changes** (path plus new value) and the upload id; the server applies them to the stored original and
  returns the same format, as an attachment with **the same file name** (no suffix, no backup). Anti-forgery token and cookie are used.
- U1 result: the zip kept the same entries (screenshot, extra info, version, the pickled log, signatures); the log changed as expected
  and the signature entry was replaced (section 4.1).
- No round-trip check is shown to the user; no backup is offered beyond a line of home-page text telling the user to back up the save.

### 3.7 Support and failure flows [JS][HTML]
- The page script has a small "ask us to support this game" widget meant for failed uploads: the user types the game name and the site
  attaches the failed save id to the request. I did not trigger it (no failing file); its exact wording is unknown.
- The `/Contact` page (not fetched again) has message, game title, optional e-mail and optional attachment fields [WF].
- A support e-mail address is in the footer.

## 4. Special handling

### 4.1 Ren'Py signing [UP][REPO]
- **What it does:** every edited Ren'Py save gets a new signature entry. The returned entry has the same shape as a real Ren'Py one (one
  line: the word "signature", a base64 public key, a base64 signature). Our synthetic input had an invented, invalid signature; the
  site replaced it with a longer, well-formed one. The other zip entries (screenshot, extra info, version, the small JSON
  metadata) came back byte-identical; the pickled log changed in size by one byte, which fits one number needing one more byte.
- **Where the key comes from:** the site's server. The page says the keys are "unique to each user" and make the file "appear to come from
  a different virtual device". The site never sees the player's own Ren'Py key file, so it cannot use that. Two saves of the same upload
  carried the **same public key** with different signatures, so the key is stable at least within one session.
- **Unknowns:** whether the key is tied to a cookie session, an IP or a Patreon account (not tested; U5 was blocked), and whether it
  survives across sessions.
- **Inference (check against the public Ren'Py source, which `docs/research/formats-sugarcube-tyrano-renpy.md` already cites):**
  Ren'Py asks "do you trust this device" when it meets a signature from a key it does not know. So the first load of a site-signed save
  probably shows that prompt once, and, if the player accepts, later site-signed saves from the same key load quietly. SavePrism
  today writes an empty signature, which means the prompt on every load.
- Another web editor, saveeditor.online, claims to re-sign with the player's own `security_keys.txt` ([WF] summary, unverified).
  A desktop app can do that locally, with no key ever leaving the PC.

### 4.2 Encryption, passwords, keys
The site states it handles "unencrypted" generic binaries and that support varies with encryption. I saw no password prompt and no key
upload in any flow I ran. The formats page lists Easy Save 2/3 and Odin without saying whether encrypted files are handled. Whether the
site has built-in game keys is **not observable** (all decoding is on the server), so I record nothing about keys.

### 4.3 Compression and encodings
Only the lists in section 2.2; no compression-specific UI exists. The paste box says it accepts normal text such as a URL-encoded string,
a Base64 string or a hex string; raw binary pasted as text must be uploaded as a file instead [HTML].

### 4.4 What it refuses
- More than one file at a time (alert). Files over 25 MB (checked in the browser; server message too). Files the browser cannot read
  (for example cloud-only files): the user is told to move the file to a local folder like Downloads.
- Free users are limited per IP address (4.5).
- Unsupported or corrupt files: a failure page with the "ask for support" widget (not seen).

### 4.5 Limits and paywall [UP][NET]
- **File size:** 25 MB per file [JS].
- **Rate limit (free tier):** after 4 uploads in about 5 minutes the 5th attempt was refused before any bytes were sent. The message said
  the maximum number of saves for the visitor's IP was reached, showed a countdown (about 3 minutes 17 seconds left), and said patrons
  have no limits. Patreon sign-in (OAuth) is the only account system [HTML].
- Patreon: "starting at $1/month", no tier names or perks visible to an automated fetch [WF]. Patron counts shown on the cards (from 82 up to
  4,276 per card) are a rough popularity proxy at best.

## 5. UX elements, ads, languages, privacy

- **Upload area:** an "Upload file" button, a "Paste code" button, and the whole page is a drop zone ("Drop files here to upload") [HTML][JS].
- **Paste code:** a text box ("Paste your Base64 or save code"), Submit and Cancel. The pasted text is uploaded as `savegame.txt`
  like a file, so it is processed on the server too. It tells users that binary-looking text must go through Upload file [HTML][JS].
- **Progress:** an upload progress bar; a "Loading..." placeholder while the editor loads.
- **Download button label** repeats the file name ("Download 1-1-LT1.save") [UP].
- **Help texts:** each engine card gives where saves live (`%LocalAppData%\<Game>\Saved\SaveGames`, `%AppData%\LocalLow\...`, the
  registry key for Unity, the Flash `#SharedObjects` folder) and a tip. The home page ends with "back up your save" advice [HTML].
- **Dark mode:** automatic from the OS setting (stored in a cookie) plus a manual toggle link [HTML][NET].
- **Languages:** English only. The pages mark keys as "do not translate" and labels as translatable, so they rely on the browser's
  automatic translation for other languages [UP].
- **Ads:** none on the home page, `/formats` or in the editor [HTML][NET]. Only Patreon links and a patron credit line on each card.
- **Accounts:** none required. "Login" is Patreon OAuth for patrons [HTML].
- **Privacy:**
  - The file is uploaded to the server and kept there (the footer says submitted data is deleted after 1 day). The upload id is a random GUID
    used in later calls. No end-to-end privacy claim exists [HTML][NET].
  - Google Analytics runs on every page (two generations of it) [NET]. There is **no cookie or consent banner**, although the site sets
    analytics cookies [NET].
  - Cookies seen: a session cookie, an anti-forgery cookie, a dark-mode flag, and the analytics cookies. No local storage or IndexedDB [REA].
  - The rate-limit message prints the visitor's IP on screen [UP].
- **Pages:** home, formats, contact. No FAQ, no help center, no changelog beyond the card dates, no ToS or privacy page.

## 6. Network flow of one edit (for reference) [NET]
1. Home page load (about 56 KB) with jQuery, Bootstrap, the upload plugin, about 15 small PNG icons and analytics.
2. `POST /UploadCheck` (empty body) returns "ready for file", or, when blocked, a result that renders the error page instead.
3. `POST /UploadSave` (multipart, field `file`) returns a GUID and the original file name.
4. `GET /SaveEdit2?id=<guid>&fileName=...` returns the editor HTML (about 64 KB for the generic editor, about 7 KB plus a 4 KB script for the RPG Maker form).
5. Generic editor only: `GET /DownloadJson/<guid>` returns the values (key-name table, rows, a small summary).
6. Save: `POST /JsonSave` (generic; id + change list) or `POST /RpgsaveFile` (RPG Maker form); the response is the file.
7. Full data view: `GET /SaveEdit2/<guid>` after a feature call.

## 7. Weak points a player would notice
1. **Privacy:** the save goes to someone else's server, is kept up to a day, tracked by analytics, with no consent banner.
2. **Free limit:** about four saves per few minutes per IP; patrons only for more. Only one file at a time, 25 MB cap.
3. **Needs internet** and works only while the site is up (site updates interrupt it).
4. **No backup, no undo, no round-trip proof.** The returned file replaces yours; the site just says to back up.
5. **Clumsy edits:** numbers through a browser prompt, one at a time; flat paths instead of a tree; no multi-select; no add/remove of
   items or keys; stat rows are modifiers, not final values; RPG Maker form has no switches (only in the full-data list).
6. **Numeric names:** "Item #5" until the player finds and uploads the game's names file.
7. **Huge numbers appear as text** (Ren'Py big ints).
8. **Ren'Py trust prompt** for foreign signing keys (likely once; section 4.1).
9. **Unknown file = dead end,** apart from a "tell us the game" form; no local diagnostics.
10. English only, no keyboard shortcuts, no theme or density settings beyond dark mode, no AI help.
11. The scope is claimed, not verifiable: the formats page lists about 70 formats, but only the ones in 2.3 were exercised.

## 8. Where SavePrism is already ahead (from README.md and the repo)
- Everything local; nothing uploaded; no limit.
- Backup before every write (app data folder), atomic replace, write read back and checked, round-trip guard before editing.
- Undo/redo, shortcuts, Backups dialog with restore, text size and color settings, theme and language (EN, TR).
- Add/remove entries (JSON, ES3, SugarCube, RPG Maker lists), "Set all" per RPG Maker list, switches in the RPG Maker view, own names for
  unnamed variables and switches, Copy code as well as Paste code, an AI bridge (Ask AI and an MCP server).
- Formats SavePrism names that the site does not name: raw Pokemon Gen 3 `.sav/.srm` (the site names only a SharkPort GBA container),
  Palworld PlZ, and Unity PlayerPrefs as a `.reg` export (the site only mentions the registry as a place saves live).

## 9. Ranked gap list: "SavePrism lacks ..."

Ranked by how many players it touches and how visible it is. Format gaps are claims from the site's formats page unless marked
"confirmed".

1. **Ren'Py saves that load without a trust prompt.** SavePrism writes an empty signature, so the game asks the player to trust every
   edited save; signed `persistent` files open read-only (REPO). Ren'Py is the biggest visual-novel audience. The site re-signs with a
   server-held key (confirmed with U1). SavePrism can do better: sign locally with the player's own key file, which never leaves the PC.
   Needs a read of the public scheme (already summarized in the repo's format notes).
2. **A fallback for unknown formats ("generic binary analyzer").** The site claims it can open unencrypted binaries it has no named
   codec for. Players with an obscure game get an answer there and a dead end here. A safe SavePrism version: a read-only "find a value"
   tool and, only if a re-read check passes, patching.
3. **Compare two saves.** The site's editors have a "difference since the previous save" mode (confirmed in the code; trigger not seen).
   It is how players find unnamed variables (Wolf RPG, Ren'Py, any game with no names). SavePrism only shows old-to-new for the current
   edit, not file against file.
4. **Unity serializer family beyond Easy Save 3 and .NET BinaryFormatter:** Easy Save 2, Odin Serializer, Adventure Creator and Pixel Crushers
   saves. Unity is the largest engine; many "Unity .dat" files are one of these.
5. **Structured text formats:** XML (big for Stardew-style saves), INI/CFG, YAML, TOML, plist, properties, CSV. Players with plain
   text saves currently get only JSON and Godot config.
6. **Python pickle files outside Ren'Py** (`.pkl`/`.pickle`, other Python engines), and binary object formats **MessagePack, CBOR, BSON,
   Protocol Buffers, NBT (Minecraft), Bitser**. SavePrism's pickle reader exists only inside the Ren'Py codec.
7. **More compression layers:** BZip2, Zstd, LZ4 for whole saves (only the Witcher chunks use LZ4 today), LZF, LZMA/XZ, Brotli, TAR,
   a plain XOR layer, and Oodle (Palworld "PlM": SavePrism refuses it today; the site claims Oodle, but Oodle is a licensed codec, so
   check the license before copying the idea). Also **hex and URL-encoded pasted codes**; today SavePrism reads LZ-String Base64 and JSON.
8. **Terraria `.plr` and `.wld`.** One of the most-edited games; named on the site.
9. **More visual-novel and indie engines:** Adventure Game Studio, Artemis, Pixel Game Maker MV, Smile Game Builder/YUKAR, KiriKiri `.ksd`,
   NScripter, AliceSoft, LiveMaker, Emuera, GameMaker, Construct Classic, Visionaire. Large for Japanese-game players.
10. **Where-is-my-save help in the app.** The site shows the folder for each engine (Unreal, Unity, Flash, RPG Maker); SavePrism has
    that text only in docs, not in the UI. Cheap and helps normal players.
11. **Text encodings** for Shift-JIS, GB18030, CP949, UTF-16/32 text values. SavePrism refuses some non-ASCII edits (RPG Maker 2000/2003)
    and reads QSP as Windows-1251 only. Japanese, Chinese and Korean saves need this.
12. **The Witcher 3 Gwent collection view** (shows missing cards). SavePrism edits orens and item quantities only.
13. **Save containers and console/emulator saves:** BioWare ERF/GFF (Mass Effect, Dragon Age, KOTOR), BND4 (FromSoftware), MPQ, PS1/PS2
    memory cards, PS2 `.psu`, Snes9x and RetroArch states, SharkPort GBA. Retro and RPG players.
14. **Key-value databases** LevelDB, RocksDB, LiteDB (SavePrism has SQLite).
15. **Names from a single picked file** (`Items.json`, `System.json`) when the save is not next to the game. SavePrism has "Choose data
    folder"; check it also accepts a single names file. Lower priority.
16. **A way to report an unsupported file** without uploading it (for example a local report with file extension, size, header hash and
    the game name, copied by the user). The site has a "tell us the game" widget; keep ours local and opt-in.

## 10. Other sites and tools worth the same analysis (no deep analysis here)
1. **saveeditor.online** (https://saveeditor.online): a second web editor with a Patreon model, Korean and English UI, a cross-device save library, pinned
   saves and version history, and claimed Ren'Py re-signing from the player's own key file. Its format list overlaps SavePrism's closely
   (including Pokemon Gen 3, QSP, Naninovel, SugarCube). Source: one fetch summary, unverified.
2. **save-editor.com** (https://www.save-editor.com/): long-running multi-game web editors, including an RPG Maker MV `.rpgsave` to text and back
   converter; reported to be ad-heavy. Automated fetch returned 403, so I know it only from a search snippet.
3. **PKHeX** (https://github.com/kwsch/PKHeX): the reference Pokemon save editor (all generations, box editing, legality checks). It is GPL, so
   study behavior only. From general knowledge, not re-checked today. SavePrism has Gen 3 only.
4. **NBTExplorer** (https://github.com/jaquadro/NBTExplorer): the standard Minecraft NBT tree editor; NBT is on the site's list and absent here.
   From general knowledge, not re-checked today.
5. **Save Wizard** (https://www.savewizard.net): paid console save editing (decrypt, edit, re-sign, cheat lists). Different platform
   but sets player expectations for "pick a cheat, apply". From general knowledge, not re-checked today.

## 11. Unknowns and what to check next
- How the free-tier limit counts (window length, whether downloads count) and whether Patreon sign-in changes the Ren'Py key.
- Whether accepting Ren'Py's trust prompt stores the site's key (check public Ren'Py source and a real game).
- The "compare with previous upload" trigger and its UI; the names-file upload step for RPG Maker; the failure page wording.
- Whether the formats page claims all work (only four editors were exercised). A later pass with more synthetic fixtures (Unity ES2,
  Odin, XML, NBT, Terraria) would show which claims hold, but each upload counts toward the site's limit; wait out the cooldown first.
