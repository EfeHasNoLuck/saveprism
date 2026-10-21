# Yol Haritası

> **TL;DR:** 9 milestone (M0–M8). Her biri tek bir **görünür demo** ile biter. Görevler 15–120 dk'lık mikro görevlerdir; her birinin ilk hamlesi ≤2 dk.
>
> **Sonraki adım:** M0-1 — kararlar (≈10 dk), [NOW.md](../NOW.md).

**Okuma anahtarı:** `[ ]` görev · ⏱ tahmini süre (ajan + sen birlikte) · ▶ ilk hamle · ✅ bitti sayılır · 👤 yalnız sen yapabilirsin

Kurallar:
1. Aynı anda **tek** görev "devam ediyor".
2. Kapsam dışı fikir → NOW.md "Park alanı" (veya GitHub `later` etiketi).
3. Her görev bitince NOW.md güncellenir ve commit atılır.
4. Tahminler M1 sonunda gerçek sürelere göre düzeltilir.
5. Çok görevli işler fabrika akışıyla yürür: `fabrikayı başlat: M1 Grup A` → dalga planı → KAPI-1 → kod → denetim → KAPI-2 ([FABRIKA.md](FABRIKA.md)).

---

## Genel bakış

| M | Ad | Demo (görünür kazanım) | Tahmin |
|---|---|---|---|
| M0 | Temel | `pnpm tauri dev` → boş ama temalı, iki dilli pencere; CI yeşil | 1 oturum |
| M1 | Çekirdek + CLI | `saveprism decode file1.rpgsave` → JSON → geri, **bayt-bayt aynı** | 2–3 oturum |
| M2 | İlk motorlar | ES3 (şifreli dahil), GVAS, RPG Maker MV/MZ altın testlerle | 3–4 oturum |
| M3 | Masaüstü MVP | Dosyayı sürükle → altını değiştir → kaydet (yedekli, doğrulamalı); tanınmayan dosyada Teşhis Raporu | 4–5 oturum |
| M4 | Arama + AI köprüsü | "AI için kopyala" → Claude'da düzenle → Patch yapıştır → fark → uygula; AI'ın önerdiği tarif otomatik doğrulanıyor | 3–4 oturum |
| M5 | İlk public sürüm v0.1.0 | GitHub Release: `.exe`/`.msi`, `.dmg`, AppImage + otomatik güncelleme | 2 oturum + 👤 onay |
| M6 | Format genişleme | PlayerPrefs, Godot, GameMaker, Ren'Py (oku → yaz → imzala), Twine | 4–6 oturum |
| M7 | Anahtar Bulucu + Profiller | Oyun klasörünü seç → motor, kayıtlar ve ES3 parolası otomatik | 3–4 oturum |
| M8 | Topluluk & AI-native | MCP sunucusu, Weblate, site, Marshal/NRBF/Palworld | sürekli |

*Oturum ≈ 2–3 saatlik odaklı çalışma.*

---

## M0 — Temel

**Hedef:** gerçek ama boş bir uygulama ve yeşil CI. **Demo:** pencere açılıyor, tema ve dil değişiyor, ilk PR'da CI yeşil.

### Grup A — Kararlar ve araçlar
1. [~] **M0-1 Kararlar** 👤 ⏱10 dk ▶ NOW.md'yi aç ✅ kararlar.md'de ADR-008…012 "Kabul" — *ADR-008, 010, 011 kabul; ADR-002 (Rust) A/B bekliyor*
2. [x] **M0-2 Skill + rea + fabrika kurulumu** ⏱30 dk ▶ kurulum talimatı ✅ 6 skill proje kapsamında, fabrika kancaları testten geçti, rea MCP kayıtlı — *kanıt: koruma.sh 18/18, status line + bağlam uyarısı çalıştı, `claude mcp get rea` → Connected (2026-10-21)*
3. [x] **M0-3 Risk kaydı + kalite stratejisi + kod haritası** ⏱45 dk ✅ [RISKS.md](RISKS.md), [QUALITY.md](architecture/QUALITY.md), [MAP.md](MAP.md) — *iş birliği notu M5-6'ya taşındı (ADR-010)*
4. [x] **M0-4 Rust araç zinciri** ⏱20 dk ▶ `winget install Rustlang.Rustup` ✅ `cargo --version` çalışıyor; MSVC Build Tools ve WebView2 doğrulandı — *kanıt: rustc/cargo 1.99.1, rustfmt 1.10.0, clippy 0.1.99; VS 2022 + 2026 MSVC; WebView2 155; `cargo run` → "Hello, world!" (2026-10-21)*
5. [ ] **M0-5 Monorepo iskeleti** ⏱30 dk ▶ pnpm + cargo workspace dosyaları ✅ `pnpm tauri dev` pencere açıyor

### Grup B — İskelet kalitesi
6. [ ] **M0-6 shadcn/ui + Tailwind v4 + tema token'ları** ⏱30 dk ▶ `pnpm dlx shadcn@latest init` ✅ açık/koyu/sistem düğmesi çalışıyor
7. [ ] **M0-7 i18n iskeleti (TR/EN)** ⏱20 dk ▶ `locales/tr/common.json` ✅ dil canlı değişiyor
8. [ ] **M0-8 CI** ⏱40 dk ▶ `.github/workflows/ci.yml` ✅ fmt/clippy/test + typecheck/lint/vitest — Windows, Linux, macOS yeşil
9. [ ] **M0-9 Topluluk dosyaları** ⏱30 dk ▶ CONTRIBUTING iskeleti ✅ CONTRIBUTING, CODE_OF_CONDUCT, SECURITY, DISCLAIMER, issue/PR şablonları
10. [ ] **M0-10 cargo-deny lisans politikası** ⏱15 dk ▶ `deny.toml` ✅ GPL bağımlılık eklenince CI kırmızı

### Grup C — Ad ve dil
11. [x] **M0-11 Yeniden adlandırma → SavePrism** ⏱20 dk ✅ dokümanlar, CLAUDE/AGENTS, skill metinleri ve plan yeni adı kullanıyor; crate adları `saveprism-*`, hata kodları `SP-*` — *kanıt: `git grep -i saveprism` yalnız ad geçmişi dosyalarında ve kilitli kanca yorumunda (2026-10-21)*
12. [ ] **M0-12 Repo dokümanlarını EN'e çevir** ⏱45 dk ▶ motor: agy (ucuz) + Claude denetimi ✅ README.md EN + README.tr.md; docs/ EN; sohbet TR kalır

---

## M1 — Çekirdek + CLI

**Hedef:** UI'dan bağımsız, test edilmiş çekirdek. **Demo:** `saveprism decode file1.rpgsave --json` → `saveprism encode` → orijinalle bayt-bayt aynı.

### Grup A — Model
1. [ ] **M1-1 SaveDoc + düğüm arenası + TypeHint** ⏱60 dk ▶ `crates/saveprism-core/src/doc.rs` ✅ birim testleri yeşil
2. [ ] **M1-2 Codec trait + Recipe + Params serileştirme** ⏱45 dk ✅ tarif TOML'a yazılıp geri okunuyor
3. [ ] **M1-3 Katmanlar: Base64, GZip, zlib, LZ-String** ⏱60 dk ✅ her katman parametreleri koruyarak round-trip
4. [ ] **M1-4 Biçim koruyan JSON codec** ⏱60 dk ✅ girinti, anahtar sırası ve `1.0` metni korunuyor
5. [ ] **M1-5 Detector v0** ⏱45 dk ✅ magic + deneme-çözme ile doğru katman zinciri ve güven puanı

### Grup B — CLI ve güvenlik
6. [ ] **M1-6 `saveprism inspect | decode | encode`** ⏱45 dk ✅ demo komutu çalışıyor
7. [ ] **M1-7 Round-trip koruması + rapor** ⏱30 dk ✅ eşleşmeyen dosya salt-okunur işaretleniyor
8. [ ] **M1-8 Güvenli yazma modülü** ⏱45 dk ✅ yedek halkası + atomik değiştirme + doğrulama testli
9. [ ] **M1-9 Girdi sınırları + ilk fuzz hedefi** ⏱30 dk ✅ sıkıştırma bombası testi panik yerine hata döndürüyor
10. [ ] **M1-10 Fixture düzeni + altın test koşucusu** ⏱30 dk ✅ `fixtures/samples/<motor>/<ad>` ekleyince test otomatik koşuyor

---

## M2 — İlk motorlar

**Hedef:** en çok istenen boşluk formatları. **Demo:** üç motordan gerçek kayıtlar altın testlerden geçiyor.

### Grup A — Codec'ler
1. [ ] **M2-1 RPG Maker MV/MZ** ⏱90 dk ✅ JsonEx meta korunuyor; `data/*.json`'dan ad sözlüğü
2. [ ] **M2-2 ES3 düz** ⏱45 dk ✅ `__type` ipuçları TypeHint'e dönüşüyor
3. [ ] **M2-3 ES3 AES + GZip** ⏱60 dk ✅ PBKDF2/AES-CBC + düz metin sezgisi; yanlış parola net hata
4. [ ] **M2-4 `saveprism keys try`** ⏱30 dk ✅ aday listesinden doğru parolayı buluyor
5. [ ] **M2-5 Unreal GVAS (uesave)** ⏱90 dk ✅ tip ipucu desteğiyle round-trip

### Grup B — Gerçek veri
6. [ ] **M2-6 Palworld `PlZ` kabuğu** ⏱45 dk ✅ zlib kabuklu GVAS açılıyor (`PlM`/Oodle → Park)
7. [ ] **M2-7 Fixture toplama** 👤 ⏱30 dk ▶ sahip olduğun oyunlardan her motor için 2 kayıt ✅ `fixtures/samples/` altında, kişisel veri temizlenmiş (bu klasör ajanlara kilitli; dosyaları sen koyarsın)
8. [ ] **M2-8 Benchmark** ⏱30 dk ✅ ≥50 MB GVAS açılış süresi ölçüldü, NOW.md'ye yazıldı

---

## M3 — Masaüstü MVP

**Hedef:** ilk gerçek kullanıcı akışı. **Demo:** sürükle → bul → değiştir → kaydet → oyunda yükle 👤.

### Grup A — Görünüm
1. [ ] **M3-1 Tauri komutları + tauri-specta bağları** ⏱60 dk ✅ open/children/edit/save TS'ten tipli çağrılıyor
2. [ ] **M3-2 Uygulama düzeni** ⏱60 dk ✅ kenar çubuğu, sekmeler, denetçi, durum çubuğu
3. [ ] **M3-3 Tembel sanal ağaç** ⏱90 dk ✅ 1M düğümlü belgede akıcı kaydırma
4. [ ] **M3-4 Tipe göre değer editörleri** ⏱60 dk ✅ int genişliği/aralık denetimi, float, bool, string
5. [ ] **M3-5 Geri al / yinele + bekleyen değişiklikler** ⏱45 dk ✅ Ctrl+Z / Ctrl+Y

### Grup B — Akış
6. [ ] **M3-6 Kaydet akışı** ⏱45 dk ✅ yedek → yaz → doğrula → "Kaydedildi · yedek alındı · doğrulandı ✓"
7. [ ] **M3-7 Sürükle-bırak + son dosyalar** ⏱30 dk
8. [ ] **M3-8 Hex görünümü** ⏱45 dk ✅ bilinmeyen dosya hex + entropi ile açılıyor
9. [ ] **M3-9 Ham JSON (Monaco, alt ağaç)** ⏱45 dk
10. [ ] **M3-10 Boş ekran + hata metinleri** ⏱30 dk ✅ 3 adımlı boş ekran; hatalar "Neden + Çözüm"

### Grup C — Teşhis Raporu v1
11. [ ] **M3-11 Rapor modeli + maskeleme** ⏱60 dk ✅ tespit izi, hata zinciri, hex başlık; kullanıcı adı/Steam ID/e-posta maskeli (birim testli)
12. [ ] **M3-12 Rapor ekranı + Kopyala** ⏱45 dk ✅ tanınmayan dosyada rapor açılıyor; Markdown panoya
13. [ ] **M3-13 Çökme yakalama** ⏱30 dk ✅ Rust panic hook + React ErrorBoundary aynı raporu üretiyor

---

## M4 — Arama + AI köprüsü

**Hedef:** bulmak ve AI ile düzenlemek kolay. **Demo:** AI ile yapılan değişiklik fark önizlemesinden geçip uygulanıyor.

### Grup A — Bulmak
1. [ ] **M4-1 Arama motoru (Rust, sorgu sözdizimi)** ⏱90 dk ✅ metin/regex/sayı/yol sorguları
2. [ ] **M4-2 Tablo görünümü** ⏱60 dk ✅ nesne dizileri tablo olarak düzenlenebiliyor
3. [ ] **M4-3 Kayıt karşılaştırma** ⏱60 dk ✅ değişen / artan / azalan filtreleri

### Grup B — AI ile düzenlemek
4. [ ] **M4-4 AI için kopyala** ⏱45 dk ✅ alt ağaç + token tahmini + komut şablonu panoya
5. [ ] **M4-5 AI'dan içe aktar** ⏱90 dk ✅ JSON veya JSON Patch → fark → tip denetimi → uygula
6. [ ] **M4-6 Ctrl+K komut paleti** ⏱30 dk

### Grup C — Teşhis Raporu v2 (AI'a sor)
7. [ ] **M4-7 "AI'a sor" seviye 1** ⏱30 dk ✅ rapor + hazır soru panoya; "format isteği" issue şablonu açılıyor
8. [ ] **M4-8 Önerilen tarifi dene** ⏱90 dk ✅ AI cevabındaki tarif bloğu bellekte deneniyor; round-trip geçerse profile kaydediliyor

---

## M5 — İlk public sürüm (v0.1.0)

**Hedef:** herkes tek tıkla kurabilsin. **Demo:** GitHub Release sayfası + uygulama içi güncelleme bildirimi.

1. [ ] **M5-1 Updater anahtarı + `tauri-plugin-updater`** ⏱30 dk ✅ eski sürüm yeni sürümü buluyor
2. [ ] **M5-2 release-please + tauri-action matrisi** ⏱60 dk ✅ etiket → üç platform paketi taslak sürümde
3. [ ] **M5-3 README (EN + TR), ekran görüntüleri, GIF** ⏱45 dk
4. [ ] **M5-4 THIRD_PARTY_NOTICES üretimi** ⏱30 dk ✅ cargo-about + pnpm licenses çıktısı pakette
5. [ ] **M5-5 Public'e alma + v0.1.0** 👤 ⏱20 dk ▶ senin açık onayın ✅ repo public, release yayınlandı
6. [ ] **M5-6 RPGSaveEditor'e merhaba notu** 👤 ⏱5 dk ▶ [taslak](research/outreach-rpgsaveeditor.md) ✅ Discussion linki NOW.md'de

Sonrası (Park): SignPath başvurusu, winget manifesti.

---

## M6 — Format genişleme

**Hedef:** "evrensel" sözünü tutmak. **Demo:** her yeni motor için bir gerçek kayıt düzenlenip oyunda yüklendi 👤.

### Grup A
1. [ ] **M6-1 Unity PlayerPrefs** (registry / plist / XML) ⏱90 dk
2. [ ] **M6-2 Godot** (şifreli + ConfigFile + `store_var` Variant) ⏱120 dk
3. [ ] **M6-3 GameMaker** (INI + ds_map dizeleri) ⏱60 dk

### Grup B
4. [ ] **M6-4 Ren'Py okuma** (ZIP + pickle → ağaç) ⏱120 dk
5. [ ] **M6-5 Ren'Py yazma + yerel imza** ⏱120 dk ✅ düzenlenen kayıt oyunda "güveniyor musun?" sormadan açılıyor
6. [ ] **M6-6 Twine/SugarCube** ⏱30 dk

---

## M7 — Anahtar Bulucu + Oyun Profilleri

**Hedef:** kullanıcı format bilmeden çalışsın. **Demo:** oyun klasörü seç → motor + kayıtlar + (gerekirse) parola otomatik.

### Grup A — Bulucular
1. [ ] **M7-1 Motor tespiti** ⏱60 dk
2. [ ] **M7-2 Kayıt konumu bulucu** ⏱45 dk
3. [ ] **M7-3 Anahtar kasası** (`keys.toml` + kanıt) ⏱45 dk
4. [ ] **M7-4 .NET `#US` okuyucu → ES3 aday üretimi** ⏱90 dk
5. [ ] **M7-5 IL2CPP string literal okuyucu** ⏱90 dk

### Grup B — Profiller
6. [ ] **M7-6 Profil şeması + doğrulayıcı + 3 örnek profil** ⏱60 dk
7. [ ] **M7-7 Oyun kütüphanesi ekranı** ⏱60 dk
8. [ ] **M7-8 Profil katkı rehberi + PR şablonu** ⏱30 dk

---

## M8 — Topluluk & AI-native (sürekli)

1. `saveprism-mcp` — MCP sunucusu (yazma her zaman onaylı); `get_last_diagnostic` = Teşhis Raporu seviye 2
2. Weblate entegrasyonu + 3 yeni dil
3. GitHub Pages sitesi + canlı "format destek tablosu"
4. RPG Maker XP/VX/VXA (`alox-48`), NRBF, RPG Maker 2000/2003, Wolf RPG, Palworld `PlM` (lisans 🔎)
5. WASM web sürümü (aynı çekirdek, tarayıcıda yerel)

---

## 👤 Yalnız senin yapabileceklerin (özet)

1. Kararlar (M0-1)
2. RPGSaveEditor'e merhaba notunu göndermek (M5-6, v0.1 sonrası)
3. Kendi oyunlarından fixture kayıtları (M2-7)
4. Düzenlenen kayıtları gerçek oyunda yükleyip test etmek (M3, M6)
5. Public'e alma ve sürüm onayı (M5-5)
