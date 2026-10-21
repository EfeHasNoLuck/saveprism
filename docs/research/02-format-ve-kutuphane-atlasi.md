# 02 — Format ve Ayrıştırıcı Atlası

> **TL;DR:** Her kayıt dosyası bir katman yığınıdır: **kapsayıcı → kodlama → şifreleme → sıkıştırma → serileştirme**. Çekirdek bu katmanları tek tek çözer ve aynı parametrelerle geri sarar. Aşağıda motor bazında katmanlar, doğrulanmış teknik ayrıntılar, kullanılacak kütüphane ve lisans durumu var.
>
> **Sonraki adım:** [ROADMAP](../ROADMAP.md) M1-3 (katman boru hattı).

Lejant: ✅ kaynak koddan / resmi README'den doğrulandı · 🔎 spike'ta doğrulanacak

---

## 1. Motorlar

### P0 — MVP

**Unreal Engine — GVAS `.sav`** ✅
- Katmanlar: [opsiyonel oyun kabuğu: Palworld `PlZ` = zlib, `PlM` = Oodle] → GVAS ikili (magic `GVAS`).
- Konum: `%LOCALAPPDATA%\<Oyun>\Saved\SaveGames\`.
- Kütüphane: `uesave` (Rust, MIT, v0.7.1) — JSON'a ve geri. Bazı Map/Set struct tipleri için **tip ipucu** ister → oyun profilinde tutulur.
- Referans: UESaveTool (Node, MIT), UeSaveGame (.NET), palworld-save-tools (Python, MIT).
- Risk: Oodle (`PlM`) tescilli sıkıştırma → opsiyonel/harici modül; açık kaynak çözücülerin lisansı 🔎.

**Unity — Easy Save 3 `.es3`** ✅
- Düz: JSON benzeri metin, `{"anahtar":{"__type":"int","value":5}}`.
- Şifreli: `dosya = salt[16] ‖ AES-128-CBC(key, IV = salt, PKCS7)`, `key = PBKDF2-HMAC-SHA1(parola, salt, 100 iterasyon, 16 bayt)`. Bazı oyunlar önce GZip uygular.
- Parola oyuna özeldir → Anahtar Bulucu ([03-rea-oyun-kitabi.md](03-rea-oyun-kitabi.md) Senaryo A).
- Doğrulama tuzağı: yanlış anahtar PKCS7 kontrolünü ~1/256 olasılıkla geçer. Ek sezgi şart: düz metin `{`, `"`, `[` ile başlar ve ≥%98 yazdırılabilir; GZip ise `1f 8b` ile başlar.
- Kütüphane: RustCrypto `aes` + `cbc` + `pbkdf2` + `sha1`, `flate2`.
- Referans: Es3Offline, unity-es3-cracker (MIT).

**RPG Maker MV / MZ** ✅
- MV `.rpgsave`: `LZString.compressToBase64(JsonEx)`.
- MZ `.rmmzsave`: zlib (pako deflate) `JsonEx`.
- JsonEx `@c` (sınıf) ve `@a` (dizi) meta alanları korunmalı; referans/döngü davranışı 🔎.
- Ad bilgisi oyunun `data/*.json` dosyalarından (Items, Actors, System) → değişken/switch adları.
- Kütüphane: `lz-str` (Rust), `flate2`. Referans: RPGSaveEditor `src/formats/mvmz` (Apache-2.0).

**Genel katmanlar** (her motorda tekrar eder): JSON, XML, INI, Base64, Hex, GZip, zlib/deflate, zstd, LZ4, LZ-String, XOR (tek/çok baytlık anahtar), AES-CBC/ECB/CFB (kullanıcı anahtarı), ZIP.

### P1

**Unity PlayerPrefs** — konumlar ✅, değer tipleri 🔎
- Windows: `HKCU\Software\<Şirket>\<Ürün>`, anahtar adları `_h<hash>` son ekli.
- macOS: `~/Library/Preferences/unity.<Şirket>.<Ürün>.plist` · Linux: `~/.config/unity3d/<Şirket>/<Ürün>/prefs` (XML).
- int/float/string kodlaması platforma göre değişir → spike.
- Kütüphane: `winreg` (Windows), `plist` (MIT).

**Godot** ✅ (Godot 4 kaynak kodu `core/io/file_access_encrypted.cpp`)
- `FileAccess.open_encrypted_with_pass`: `GDEC` magic → MD5(düz metin)[16] → uzunluk u64 → IV[16] → AES-256-CFB veri. Anahtar = `md5_text(parola)` yani 32 ASCII bayt. Godot 3 farklı 🔎.
- Düz biçimler: JSON, `ConfigFile` (INI benzeri), `store_var` ikili Variant, `.tres` metin kaynak.
- Parola bulma zinciri: KeyDot ile PCK/betik anahtarı → GDRE ile GDScript çöz → `open_encrypted_with_pass(..., "parola")` çağrısını bul.

**GameMaker** 🔎: `ini_open` INI; `ds_map_write`/`ds_list_write` hex/Base64 dizeleri; `json_stringify`; oyuna özel `buffer` kayıtları.

**Ren'Py `.save`** ✅ (Ren'Py kaynak kodu `renpy/savetoken.py`, `renpy/savelocation.py`)
- ZIP içeriği: `log` (pickle), `json`, `extra_info`, `screenshot.png`, `renpy_version`, 8.1+ için `signatures`.
- İmza: ECDSA NIST P-256; anahtarlar Ren'Py token dizinindeki `security_keys.txt`. İmza yoksa/eşleşmezse oyun "bu kayda güveniyor musun?" diye sorar.
- Yaklaşımımız: pickle **asla çalıştırılmaz** — opcode'lar ağaca çevrilir; skaler düzenlemeler bayt yamasıyla yazılır (protokol 4+ `FRAME` uzunlukları güncellenir); kullanıcı izniyle yerel `security_keys.txt` ile yeniden imzalanır.
- Referans: RPGSaveEditor `src/formats/pickle`, `src/formats/renpy` (Apache-2.0, kayıpsız yazıcı + rollback), fickling (LGPL, yalnız araştırma), unrpyc (değişken adları, yalnız araştırma).

**Twine/SugarCube** 🔎: `.save` = LZ-String Base64 JSON (durum geçmişi).

### P2

| Motor/Format | Yaklaşım |
|---|---|
| RPG Maker XP/VX/VX Ace (`.rxdata/.rvdata/.rvdata2`) | Ruby Marshal 4.8 → `alox-48` (Rust, MPL-2.0). Ad verisi için şifreli arşivler `Game.rgssad/rgss2a/rgss3a` |
| RPG Maker 2000/2003 (`.lsd`) | LCF ikili; referans EasyRPG liblcf (lisans 🔎) |
| Unity BinaryFormatter (NRBF) | [MS-NRBF] açık spesifikasyon; **kendi** Rust ayrıştırıcımız (referans: .NET 9 `System.Formats.Nrbf`, MIT). ⚠️ `nrbf-parser` crate'i GPL-3.0 → kullanılamaz |
| Wolf RPG, Naninovel `.nson`, TyranoScript, Flash `.sol` (AMF), SQLite, MessagePack/BSON/CBOR | Talep sırasına göre |

---

## 2. Çekirdeğe girecek kütüphaneler (lisans süzgecinden geçmiş)

| İhtiyaç | Seçim | Lisans | Not |
|---|---|---|---|
| GVAS | `uesave` 0.7.1 | MIT | |
| AES / PBKDF2 / SHA1 / MD5 | RustCrypto `aes`, `cbc`, `cfb-mode`, `pbkdf2`, `sha1`, `md-5` | MIT/Apache-2.0 | |
| gzip / zlib | `flate2` | MIT/Apache-2.0 | |
| zstd / LZ4 | `zstd`, `lz4_flex` | BSD-3 / MIT | |
| LZ-String | `lz-str` | MIT/Apache-2.0 | |
| Ruby Marshal | `alox-48` | MPL-2.0 | Dosya düzeyinde copyleft; bağımlılık olarak uygun |
| Pickle | Kendi opcode okuyucumuz | — | `serde-pickle` yalnız basit tipleri karşılıyor; Ren'Py sınıf referansları için özel gerek |
| NRBF | Kendi | — | GPL crate'ten kaçınıyoruz |
| İkili bildirimsel ayrıştırma | `binrw` 0.15 | MIT | Kaitai'nin Rust üreticisi olgun değil (`kaitai` 0.1.2, 2021) |
| plist / msgpack | `plist`, `rmpv` | MIT | |
| Rust → TS tip üretimi | `tauri-specta` | MIT | Tauri 2 uyumlu sürüm 🔎 |

**Kural (cargo-deny ile zorlanır):** GPL / AGPL / LGPL / SSPL yasak. Apache-2.0 ve MPL-2.0 NOTICE ile serbest.

---

## 3. Araştırma araçları (uygulamaya gömülmez)

1. **rea** — aşağıdakileri ajan üzerinden orkestre eder ([03](03-rea-oyun-kitabi.md)).
2. **Kaitai Struct / ImHex pattern** — bilinmeyen ikili formatı belgelerken (`.ksy` / `.hexpat`). ImHex GPL-2 → yalnız harici araç.
3. **CyberChef** (Apache-2.0) — "Magic" işlemi (entropi + imza tabanlı katman tahmini) Detector'ımızın ilham kaynağı; kod gömülmez.
4. **Godot:** gdsdecomp/GDRE, KeyDot · **Unity:** dnSpyEx/ILSpy (Mono), Il2CppDumper (IL2CPP) · **Python:** fickling (pickle analizi).
5. **Python script'leri** — palworld-save-tools vb. ile çapraz doğrulama (yalnız geliştirme ortamında).

---

## 4. Tespit sinyalleri (Detector başlangıç seti)

| İmza | Anlam |
|---|---|
| `GVAS` (ofset 0) | Unreal save |
| `PlZ` / `PlM` başlıklı kabuk | Palworld (zlib / Oodle) |
| `PK\x03\x04` + `log` girdisi | Ren'Py |
| `GDEC` | Godot şifreli dosya |
| `1f 8b` | GZip |
| `78 01` / `78 9c` / `78 da` | zlib |
| `28 b5 2f fd` | zstd |
| `04 22 4d 18` | LZ4 frame |
| `04 08` | Ruby Marshal 4.8 |
| `80 02`…`80 05` | Pickle protokol 2–5 (ZIP dışında) |
| `00 01 00 00 00 ff ff ff ff 01 00 00 00 00 00 00 00` | NRBF SerializationHeader |
| Yüksek entropi + boyut ≡ 0 (mod 16) + boyut ≥ 32 | Blok şifre adayı (ör. ES3 AES) |
| Base64 alfabesi | Base64 / LZ-String Base64 → deneme-çözme + JSON doğrulama |

---

## Kaynaklar

- [Godot `file_access_encrypted.cpp`](https://github.com/godotengine/godot/blob/master/core/io/file_access_encrypted.cpp)
- [Ren'Py `savetoken.py`](https://github.com/renpy/renpy/blob/master/renpy/savetoken.py) · [`savelocation.py`](https://github.com/renpy/renpy/blob/master/renpy/savelocation.py)
- [cppbear/Es3Offline](https://github.com/cppbear/Es3Offline) (ES3 şifreleme şeması)
- [trumank/uesave](https://github.com/trumank/uesave) · [Speak2Erase/alox-48](https://github.com/Speak2Erase/alox-48)
- [Titoot/KeyDot](https://github.com/Titoot/KeyDot) · [Baconing/godot-save-editor](https://github.com/Baconing/godot-save-editor)
- [kaitai-io/kaitai_struct_formats](https://github.com/kaitai-io/kaitai_struct_formats) · [trailofbits/fickling](https://github.com/trailofbits/fickling)
- crates.io sürüm/lisans bilgileri (2026-10-21)
