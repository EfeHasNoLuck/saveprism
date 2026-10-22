# TASKS: SavePrism

Durum: `[ ]` bekliyor · `[~]` dalgada · `[x]` doğrulandı (kanıtlı) · `[!]` durdu (3 deneme) · `[?]` karar bekliyor
Alan sırası sabittir. Kart dosyası: `.fabrika/gorevler/<ID>.md` (CEO, dalga onaylanınca yazar). Rapor: `.fabrika/raporlar/<ID>.md`.
Kural: aynı dalgada iki görevin `Sahip` listesi kesişmez. Kilitli kaynaklar yalnız `[ben]` görevinde.
Talep kaynağı: `docs/ROADMAP.md` (görev ID'leri `M<n>-<k>`). Durumun tek ekranlık özeti: `NOW.md`.

---

## M0 Grup B — İskelet kalitesi (M0-5 ön koşul olarak D1'de)

- **Talep (kullanıcı, 2026-10-21):** "fabrikayı başlat: M0 Grup B"
- **Zincir:** kısa (yığın MIMARI §1'de kabul edilmiş; yeni mimari karar yok).
- **Yaklaşım (A, önerilen):** resmi üreteçleri (`create-tauri-app`, `shadcn` CLI) CEO Bash ile koşar (A1, A2); ajanlar üretilen iskeleti kartla
  uyarlar. Kilit dosyaları (`pnpm-lock.yaml`, `Cargo.lock`) yalnız A adımlarında ve CEO doğrulamasında oluşur.
  **B (elenen):** ajanlar her dosyayı elle yazar. Daha yavaş, Tauri 2 yapılandırma hatası riski yüksek, ikon (ikili PNG) üretemezler.
- **KAPI-1 (kullanıcı, 2026-10-21):** "CI için A iyi ya öyle devam edelim, eğer bizi kesmeye başlarlarsa da B kısmına geçeriz." → plan + bağımlılık listesi onaylı; CI private GitHub repoda.
- **Kapsam dışı (bu grupta):** ICU mesaj eklentisi (M3'te gerekirse), otomatik güncelleme (M5), Zustand/TanStack/Monaco (M3).
- **Bağımlılıklar:** KAPI-1'de onaylanan liste NOW.md'de değil, kartlarda yazılı; listede olmayan paket eklenmez.

### D1 — İskelet + bağımsız dosyalar · KAPI-1: onaylandı 2026-10-21

- [x] **A1 Üreteç + kurulum** (CEO, Bash) — `pnpm install` (44 paket) ve `pnpm build` geçti; Cargo.lock rust-version 1.99 ile üretildi
  - Sahip: `apps/desktop/**` (yalnız ilk üretim, M0-5'ten önce) · `pnpm-lock.yaml` · `Cargo.lock`
  - Adımlar: 1) `apps/` içinde `pnpm create tauri-app` (react-ts, pnpm) → şablon demo dosyalarını sil · 2) M0-5 bitince
    `pnpm install` → `pnpm build` → `bash .claude/araclar/kontrol.sh`

- [x] **M0-5 Monorepo iskeleti + D2 sözleşmesi**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `Cargo.toml` · `package.json` · `pnpm-workspace.yaml` · `rust-toolchain.toml` · `clippy.toml` ·
    `apps/desktop/{package.json,index.html,vite.config.ts,tsconfig*.json}` · `apps/desktop/src/{main.tsx,App.tsx,index.css,vite-env.d.ts}` ·
    `apps/desktop/src/theme/index.tsx` (taslak) · `apps/desktop/src/i18n/index.tsx` (taslak) · `apps/desktop/src-tauri/**` · `crates/saveprism-core/**`
  - Okur: MIMARI §1, §5 · QUALITY §1 · AGENTS.md
  - Bağımlı: A1 adım 1
  - Kabul: `pnpm tauri dev` "SavePrism" başlıklı pencere açar · `kontrol.sh` rust (fmt/clippy/test) + web (typecheck) çıkış 0 ·
    workspace lint'leri `unwrap`/`expect`/`panic`/`unsafe` yasaklar · Tailwind v4 bağlı · `@/` takma adı çalışır ·
    D2 sözleşmesi: `@/theme` (applyStoredTheme, ThemeProvider, ThemeToggle) ve `@/i18n` (initI18n, LanguageSwitcher) taslakları main/App'e bağlı
  - Doğrulama: CEO `kontrol.sh hepsi` · Sen `pnpm tauri dev` → pencere açılıyor (KAPI-2)
  - Denetim: denetci (dalga) · Kanıt: `kontrol.sh hepsi` → 4/4 (fmt, clippy -D warnings, test, typecheck), 2026-10-21 ·
    düzeltme turu 1: rust-version 1.99, `publish = false`, `devCsp: null` · KAPI-2: kullanıcı "tam dediğin gibi bir şey açıldı" (tarayıcıda)

- [x] **M0-9 Topluluk dosyaları**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `CONTRIBUTING.md` · `CODE_OF_CONDUCT.md` · `SECURITY.md` · `DISCLAIMER.md` · `.github/ISSUE_TEMPLATE/**` · `.github/PULL_REQUEST_TEMPLATE.md`
  - Okur: AGENTS.md · docs/RISKS.md §3 · MIMARI §6, §8 · QUALITY §2, §4 · `.claude/skills/no-ai-slop/SKILL.md`
  - Bağımlı: —
  - Kabul: 4 belge + 3 issue formu (hata, özellik, yeni format/oyun) + PR şablonu · EN, no-ai-slop · temiz oda, fixture izni ve kapsam dışı
    listesi yazılı · iletişim adresi yer tutucu (`TODO(maintainer)`, karar M5 öncesi sende)
  - Doğrulama: CEO okur · Sen 5 dk göz at (KAPI-2)
  - Denetim: denetci (dalga) · Kanıt: CoC yapısı resmi Contributor Covenant 2.1 ile aynı (7 bölüm, 4 yaptırım seviyesi, atıf) ·
    düzeltme turu 1: repo adresi, zip notu · kullanıcı göz atması isteğe bağlı (public öncesi)

- [x] **M0-10 cargo-deny lisans politikası**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `deny.toml` · `.github/workflows/deny.yml`
  - Okur: AGENTS.md kural 5 · kararlar.md ADR-006
  - Bağımlı: —
  - Kabul: izinli lisans listesi (MIT, Apache-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0, MPL-2.0, …); GPL/AGPL/LGPL/SSPL reddedilir ·
    bilinen güvenlik açığı = hata · bilinmeyen kayıt/git kaynağı = hata
  - Doğrulama: CEO `cargo deny check` → 0 · geçici, GPL lisanslı yerel bir crate eklenince → hata (sonra geri alınır)
  - Denetim: denetci (dalga) · Kanıt: `cargo deny check` → advisories ok, bans ok, licenses ok, sources ok · geçici `GPL-3.0-only`
    crate → `error[rejected]` (geri alındı), 2026-10-21

### D2 — Tema, dil, CI · KAPI-1: onaylandı 2026-10-21 · Bağımlı: D1 doğrulandı

- [x] **A2 Bağımlılık + shadcn** (CEO, Bash) — kuruldu; shadcn 4.22 (radix-nova) listede olmayan 3 paket ekledi: `cn` (MIT, shadcn'in
  clsx + tailwind-merge yerine geçen paketi), `shadcn` (MIT, CSS için), `@fontsource-variable/geist` (OFL-1.1 font) → kullanıcı kabul etti (2026-10-21); sürümde OFL metni üçüncü taraf lisanslarına eklenecek (M5)
  - Sahip: `apps/desktop/package.json` (yalnız bağımlılık satırları) · `pnpm-lock.yaml` · `apps/desktop/components.json` ·
    `apps/desktop/src/lib/utils.ts` · `apps/desktop/src/components/ui/**` · `apps/desktop/src/index.css` (yalnız ilk üretim)
  - Adımlar: 1) onaylı D2 paketlerini `pnpm add` · 2) `shadcn init` (Radix) + `shadcn add button dropdown-menu`

- [~] **M0-6 Tema: açık / koyu / sistem**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `apps/desktop/src/theme/**` · `apps/desktop/src/index.css` · `apps/desktop/src/components/ui/**`
  - Okur: MIMARI §4 madde 4 · `src/main.tsx`, `src/App.tsx` (yalnız okur)
  - Bağımlı: A2
  - Kabul: düğme açık/koyu/sistem arasında canlı geçiş · "sistem" seçiliyken işletim sistemi teması değişince canlı izler ·
    seçim yeniden açılışta korunur · renkler yalnız token'lardan · `src/theme/theme.test.ts` (saf çözümleme fonksiyonu)
  - Doğrulama: CEO `kontrol.sh web` · Sen `pnpm tauri dev` → 3 kip + yeniden açılış (KAPI-2)
  - Denetim: denetci (dalga; 1 YUKSEK düzeltildi) · Kanıt: `kontrol.sh hepsi` yeşil (typecheck, lint, 17 test, Rust 3/3) · CEO tarayıcıda:
    Açık seçildi → `saveprism.theme=light`, yenilemeden sonra açık kaldı; "sistem" açılışta OS temasını alıyor. Canlı OS takibi kullanıcıda

- [x] **M0-7 i18n iskeleti (TR/EN)**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `apps/desktop/src/i18n/**` · `apps/desktop/locales/**` · `apps/desktop/src/App.tsx`
  - Okur: MIMARI §4 madde 5 · `src/main.tsx` · `src/components/ui/dropdown-menu.tsx` (yalnız okur)
  - Bağımlı: A2
  - Kabul: dil seçici TR/EN arasında yeniden yüklemeden canlı geçiş · ilk açılışta işletim sistemi dili · seçim korunur ·
    `<html lang>` güncellenir · `App.tsx`'te sabit metin kalmaz · `src/i18n/i18n.test.ts`: tr ve en anahtar kümeleri aynı
  - Doğrulama: CEO `kontrol.sh web` · Sen `pnpm tauri dev` → dil değiştir + yeniden aç (KAPI-2)
  - Denetim: denetci (dalga; 1 YUKSEK düzeltildi) · Kanıt: testler yeşil · CEO tarayıcıda: ilk açılış TR (OS dili), English seçilince metin
    canlı değişti, `<html lang="en">`, yenilemeden sonra EN kaldı

- [x] **M0-8 CI**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `.github/workflows/ci.yml` · `.github/dependabot.yml` · `apps/desktop/eslint.config.js` · `apps/desktop/vitest.config.ts` ·
    `package.json` (yalnız scripts) · `apps/desktop/package.json` (yalnız scripts)
  - Okur: QUALITY §4 · `.claude/araclar/kontrol.sh` (yalnız okur) · `rust-toolchain.toml`
  - Bağımlı: A2
  - Kabul: yerelde `kontrol.sh` typecheck + lint + test çalıştırır, çıkış 0 · CI: Rust fmt/clippy/test Windows + Linux + macOS,
    web typecheck/lint/test · yalnız okuma izinleri · Dependabot (cargo, npm, actions) haftalık
  - Doğrulama: CEO `kontrol.sh hepsi` · CI yeşil: GitHub'da repo varsa (KAPI-1 kararı)
  - Denetim: denetci (dalga) · Kanıt: yerelde `kontrol.sh` artık typecheck + lint + test koşuyor, hepsi yeşil · GitHub CI run 39625162997:
    web ✓ · rust ubuntu-24.04 ✓ · rust windows-latest ✓ · rust macos-latest ✓ (2026-10-21, ilk denemede)

---

## M1 — Çekirdek + CLI

- **Talep (kullanıcı, 2026-10-21):** "handoff/ içindeki en yeni HANDOFF dosyasını oku, "Tam sonraki adım" 1'den devam et." → HANDOFF-20261021-1 adım 2: M1 dalga planı.
- **Demo / kabul çekirdeği:** `saveprism decode file1.rpgsave -o f.json` → `saveprism encode f.json --recipe f.recipe.toml -o out.rpgsave` → `out.rpgsave` orijinalle bayt-bayt aynı.
- **Zincir:** tam (sözleşme tasarımı: MIMARI §3.2–3.5, ADR-016).
- **Yaklaşım (A, önerilen): önce imza.** D3 tek görevle bütün ortak tipleri ve sonraki modüllerin imzalarını (boş gövde) yazar; D4 ve D5'te
  görevler dosya düzeyinde ayrılır, paralel koşar. **B (elenen):** her modül kendi tipini getirir, sırayla. 5–6 dalga sürer, aynı tipler
  iki kez yazılır, M2 codec'leri tutarsız bir tabana oturur.
- **Kapsam dışı:** RPG Maker JsonEx (`@c`/`@a`) ve MZ zlib ayrıntısı (M2-1), şifre katmanları (M2-3), JSON yama yazımı (span ile,
  M3/M4), `ReplaceFileW` (ADR-016), tracing log halkası (M3).
- **Bağımlılıklar (KAPI-1'de onay; hepsi MIT ve/veya Apache-2.0, `cargo info` ile doğrulandı 2026-10-21):** core: `thiserror` 2.0 ·
  `serde` 1.0 · `toml` 1.1 · `base64` 0.23 · `tracing` 0.1 · `tempfile` 3.27 — codecs: `flate2` 1.1 (miniz_oxide: MIT/Zlib/Apache) ·
  `lz-str` 0.2 — cli: `clap` 4.6 · `tracing-subscriber` 0.3 — test: `proptest` 1.12 · `insta` 1.49 (Apache-2.0) — araç: `cargo-fuzz`
  + `libfuzzer-sys` (NCSA ek; `fuzz/` workspace dışı, dağıtılmaz).
- **KAPI-1 (kullanıcı, 2026-10-21):** "Senin dediklerinde değiştirmek istediğim bir şey göremedim." → M1 planı + bağımlılık listesi onaylı.
  Aynı mesajda: "kişi … istediği klasöre de değiştirilmiş dosyayı çıktı alır. İsterse üstüne de yazabilir, isterse de yedek aldırabilir
  … (30 gün içinde de bu yedek veri silinir)" → M1-8 yedek politikası D4'ten önce netleşecek (karar bekliyor).

### D3 — Sözleşme · KAPI-1: onaylandı 2026-10-21

- [x] **M1-1 Çekirdek sözleşmesi (M1-1 + M1-2)**
  - Motor: claude/kodcu·sonnet · Effort: high · Deneme: 0/3
  - Sahip: kök `Cargo.toml` (yalnız `[workspace.dependencies]` + `exclude = ["fuzz"]`) · `crates/saveprism-core/**` ·
    `crates/saveprism-codecs/{Cargo.toml,src/lib.rs,src/json/mod.rs,src/layers/mod.rs}` (iskelet) · `.fabrika/raporlar/M1-1.md`
  - Okur: MIMARI §3.1–3.5 · QUALITY §1 · ADR-016 · AGENTS.md
  - Bağımlı: KAPI-1
  - Kabul: modüller `error` (SP-CORE kodları, bağlam zinciri, i18n anahtarı) · `doc` (arena, Value, Node, lexeme, TypeHint, flags) · `path` ·
    `edit` (+ `apply_edits`) · `params` · `recipe` (TOML yaz/oku) · `codec` (Layer, Codec, Confidence, ctx) · `registry` · `limits` ·
    `pipeline` (çalışır); `detect`, `guard`, `io` son imzalarıyla boş gövde (SP-CORE "henüz yok" hatası) · testler: tarif TOML'a yazılıp
    aynen geri okunuyor, bayt parametresi `base64:` ile geri geliyor, `set` lexeme'i siliyor, `Path` çözümü, sahte 2 katman + sahte
    codec ile boru hattı round-trip · `unwrap`/`expect`/`panic` yok · `kontrol.sh rust` çıkış 0
  - Doğrulama: CEO `kontrol.sh rust` · Tauri `core_version` hâlâ derleniyor
  - Denetim: denetci (KRITIK/YUKSEK yok; 2 ORTA, 4 DUSUK) + Codex ikinci göz (1 KRITIK döngülü grafik kurulamıyor, 3 YUKSEK: sınırlar
    işbirliğine bağlı, Map anahtarı adreslenemiyor, `tree_eq` Ref hedefini karşılaştırmıyor) → düzeltme turu 1 (deneme sayılmaz) uygulandı; Codex yeniden kontrol: 4 kapandı, 2 kısmen (bilinçli bedel, ADR-016), yeni
    KRITIK/YUKSEK yok;
    Object anahtar aralıkları ertelendi (codec tarar) · Kanıt: CEO `cargo fmt --all` → `kontrol.sh rust`
    3/3 ilk denemede (fmt, clippy -D warnings, test) · `cargo test --workspace` → core 103 + codecs 1 geçti · Tauri crate derlendi ·
    `cargo deny check` çıkış 0 (yalnız tekrar eden sürüm uyarısı) · düzeltme turu sonrası yine 3/3, core 132 + codecs 1 test (2026-10-21)

- [x] **CI-1 Belge değişikliğinde CI koşmasın**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `.github/workflows/ci.yml`
  - Okur: —
  - Bağımlı: KAPI-1
  - Kabul: `push` ve `pull_request` tetiklerine `paths-ignore`: `docs/**`, `**/*.md`, `handoff/**`, `.fabrika/**`; başka değişiklik yok
  - Doğrulama: CEO diff'e bakar · yalnız belge değiştiren sonraki push'ta GitHub'da CI koşusu açılmaz
  - Denetim: yok (küçük iş) · Kanıt: CEO `git diff` → yalnız `on.push` ve `on.pull_request` altına 4 satırlık `paths-ignore` eklendi,
    işler aynen (2026-10-21) · GitHub'da belge-push denemesi: `abd1a4f..eb4e2d2` (yalnız NOW, TASKS, ROADMAP) push'unda hiç koşu
    açılmadı (`gh run list`, 2026-10-22)

### D4 — Katmanlar, JSON, koruma, güvenli yazma · KAPI-1: onaylandı 2026-10-21 (M1-8 yedek politikası: A, kullanıcı 2026-10-21 "A seçeneği ile ilerleyelim") · Bağımlı: D3 doğrulandı

- [x] **M1-3 Katmanlar: Base64, GZip, zlib, LZ-String**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 2/3 (1: ilk derleme; 2: bağımsız JS dosyasında lz-str dolgusu 1 bayt fazla, 2026-10-22; CEO kararı: aynı motor/effort)
  - Sahip: `crates/saveprism-codecs/src/layers/**` · `.fabrika/raporlar/M1-3.md`
  - Okur: MIMARI §3.1, §3.3, §3.4 · `crates/saveprism-core/src/{codec,params,limits,error}.rs`
  - Bağımlı: D3
  - Kabul: `encoding.base64` (standart / URL-güvenli, dolgu, satır kırma ve sondaki satır sonu korunur) · `compression.gzip` (başlık alanları
    korunur; seviye 0–9 denenir, bulunamazsa `exact = false`) · `compression.zlib` (aynı) · `compression.lz-string` (base64 / uri; utf16 M2;
    açma kendi sınırlı portumuz çünkü lz-string açması karesel büyüyebilir ve `lz-str` sınır almaz; sıkıştırma `lz-str`) · açma
    `read_bounded` ile (aşım → `CORE_LIMIT_DECODED`, panik yok) · her katman parametreleriyle round-trip testi
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga-şefi denetçisi (KRITIK 1 + YUKSEK 1 derleme kırıcı, düzeltildi; kalan DUSUK) + Codex ikinci göz (1 YUKSEK lz-string varyantı, 2 ORTA; turu 3 ile düzeltildi) · Kanıt: Rust kontrolü 3/3, codecs 72 + core 139 test, cargo deny 0 (2026-10-21)
    · turu 4 (2026-10-22): orijinal JS ile üretilen kayıtlarda `lz-str` base64 dolgusu 1 bayt fazlaydı → JS kuralı; 8 JS vektörü testi +
    4 `.rpgsave` decode → encode birebir aynı; toplam 255 test, Rust kontrolü 3/3

- [x] **M1-4 Biçim koruyan JSON codec**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 1/3 (ilk derleme: kartın üs eşiği hatası + test verisi; CEO kararı: aynı motor/effort)
  - Sahip: `crates/saveprism-codecs/src/json/**` · `.fabrika/raporlar/M1-4.md`
  - Okur: MIMARI §3.2–3.4 · ADR-016 madde 2, 5 · `crates/saveprism-core/src/{doc,codec,params,limits,error}.rs`
  - Bağımlı: D3
  - Kabul: kendi ayrıştırıcısı (serde_json yok; derinlik sınırı, yığın taşmasız) · anahtar sırası ve tekrar eden anahtar korunur · kanonik
    olmayan sayı/metin `lexeme`'de · `JSON:int` / `JSON:float` ipucu · biçim profili: sıkışık (`JSON.stringify`), 2 / 4 boşluk, sekme,
    CRLF, `": "`, sondaki satır sonu, BOM → hepsi bayt-bayt aynı (testler) · düzensiz boşluk → ağaç eşit (test belgeler) · düzenlenmiş
    `1.0` → `2.0` · `"1.0"` metni korunur
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga-şefi denetçisi (KRITIK 1 + YUKSEK 1 derleme kırıcı, düzeltildi; kalan DUSUK) + Codex ikinci göz (1 YUKSEK lz-string varyantı, 2 ORTA; turu 3 ile düzeltildi) · Kanıt: Rust kontrolü 3/3, codecs 72 + core 139 test, cargo deny 0 (2026-10-21)

- [x] **M1-7 Round-trip koruması + rapor**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-core/src/guard.rs` · `.fabrika/raporlar/M1-7.md`
  - Okur: MIMARI §3.4 · `crates/saveprism-core/src/{pipeline,codec,recipe,error}.rs`
  - Bağımlı: D3
  - Kabul: adım başına sonuç (birebir / anlamsal / farklı) + en kötüsü genel sonuç + neden (i18n anahtarı + teknik ayrıntı) · farklı →
    `editable = false` · sahte katmanlarla üç durumun testi
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga-şefi denetçisi (KRITIK 1 + YUKSEK 1 derleme kırıcı, düzeltildi; kalan DUSUK) + Codex ikinci göz (1 YUKSEK lz-string varyantı, 2 ORTA; turu 3 ile düzeltildi) · Kanıt: Rust kontrolü 3/3, codecs 72 + core 139 test, cargo deny 0 (2026-10-21)

- [x] **M1-8 Güvenli yazma modülü**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-core/src/io.rs` · `crates/saveprism-core/tests/io.rs` · `.fabrika/raporlar/M1-8.md`
  - Okur: MIMARI §3.5 · AGENTS.md kural 1 · `crates/saveprism-core/src/error.rs`
  - Bağımlı: D3
  - Kabul: var olan dosyanın üstüne her yazışta yedek (kapatılamaz); yeni dosyada yedek yok · yedek uygulama veri dizininde (`<kök>/<fnv64(yol)>/<zaman>.bak`; 30 günden eski ve 10 fazlası silinir, en yenisi kalır) · geçici dosya → `sync_all` → `persist` · doğrulama
    başarısızsa yedekten geri alır · `default_backup_root()` Windows / macOS / Linux · testler: yedek oluşuyor, halka 10'da kalıyor,
    doğrulama hatasında orijinal bayt geri geliyor, salt-okunur hedef panik değil hata
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok) · Kanıt: Rust kontrolü 3/3 (fmt, clippy, test) + sentetik demo: base64 kaydı decode→encode bayt-bayt aynı, üstüne yazmada yedek oluştu (2026-10-22) · açık: 2 ORTA (CLI doğrulama kapanı hata kodu, Unix izin kaybı), 5 DUSUK → .fabrika/raporlar/D5b-denetim.md

### D5 — Dedektör, CLI, sınırlar, altın test · KAPI-1: onaylandı 2026-10-21 · Bağımlı: D4 (M1-5, M1-9, M1-10 boru hattında M1-8 beklenirken; M1-6 M1-8 sonrası)

- [x] **M1-5 Detector v0**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-core/src/detect.rs` · `crates/saveprism-codecs/tests/detect.rs` · `.fabrika/raporlar/M1-5.md`
  - Okur: MIMARI §3.1, §3.3 · `crates/saveprism-core/src/{codec,registry,pipeline,limits}.rs`
  - Bağımlı: D4
  - Kabul: sniff sırası + deneme-çözme + geri izleme; katman ve deneme sayısı sınırlı; hiçbiri uymazsa SP-DET hatası · testler (girdiler
    testte katmanlarla üretilir): JSON, base64(JSON), gzip(JSON), lz-string(JSON) (base64 sanılmaz), base64(gzip(JSON)) → doğru tarif
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga-şefi denetçisi (1 YUKSEK + 1 ORTA düzeltildi) · Kanıt: Rust kontrolü 3/3; tests/detect.rs 7 test (JSON, base64, gzip, zlib, lz-string, base64+gzip, rastgele) + core 146 (2026-10-21) · açık DUSUK: ön cephe bellek bütçesi (16 × max_decoded) → M1-6 dalgasında düzeltilecek

- [x] **M1-6 `saveprism inspect | decode | encode`**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-cli/**` · `.fabrika/raporlar/M1-6.md`
  - Okur: MIMARI §3.1–3.5 · `crates/saveprism-core/src/lib.rs` ve imzaları
  - Bağımlı: D4 (dedektör imzası D3'te)
  - Kabul: `inspect` (tarif, güven, round-trip sonucu; `--json`) · `decode -o f.json` (+ `f.recipe.toml`) · `encode f.json --recipe … -o …`
    (güvenli yazma + yazım sonrası doğrulama) · hata = `SP-…` kodu + bağlam, çıkış 1 · süreç içi testler: base64+JSON ve lz-string+JSON
    için decode → encode bayt-bayt aynı
  - Doğrulama: CEO demo komutları sentetik dosyayla · Sen gerçek MV kaydınla (KAPI-2)
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok) · Kanıt: Rust kontrolü 3/3 (fmt, clippy, test) + sentetik demo: base64 kaydı decode→encode bayt-bayt aynı, üstüne yazmada yedek oluştu (2026-10-22) · açık: 2 ORTA (CLI doğrulama kapanı hata kodu, Unix izin kaybı), 5 DUSUK → .fabrika/raporlar/D5b-denetim.md
    · bağımsız doğrulama (kullanıcının oyunu yok, 2026-10-22): `lz-string` 1.5.0 (orijinal JS) ile üretilen 4 MV kaydı `saveprism decode` →
    `saveprism encode` → `cmp` birebir aynı; `inspect` iki adımda `exact` (M1-3 turu 4 sonrası)

- [x] **M1-9 Girdi sınırları + ilk fuzz hedefi**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `fuzz/**` · `.github/workflows/fuzz.yml` · `crates/saveprism-codecs/tests/limits.rs` · `.fabrika/raporlar/M1-9.md`
  - Okur: QUALITY §2 · `crates/saveprism-core/src/limits.rs`
  - Bağımlı: D4
  - Kabul: gzip/zlib bombası, aşırı derin JSON, aşırı büyük girdi → panik yerine SP hatası · proptest: rastgele bayt her decode'a ve
    dedektöre → panik yok · `fuzz/` hedefleri (detect, json, layers) · fuzz.yml: haftalık + elle + `crates/**` push'unda, Linux, 60 sn/hedef
  - Doğrulama: CEO `kontrol.sh rust` · fuzz işi GitHub'da bir kez yeşil
  - Denetim: dalga-şefi denetçisi (1 YUKSEK + 1 ORTA düzeltildi) · Kanıt: tests/limits.rs 7 test yeşil (bombalar, derinlik, girdi, düğüm, proptest) · fuzz/ ve fuzz.yml yazıldı; yerelde nightly yok, fuzz koşusu ilk push'ta GitHub'da (2026-10-21)
    · GitHub fuzz koşusu 39700556640 (commit abd1a4f) yeşil: json 2,82 M · detect 1,16 M · layers 1,61 M deneme, her biri 61 sn, çökme yok;
    aynı push'ta ci (ubuntu, windows, macos, web) ve cargo-deny yeşil (2026-10-22)

- [x] **M1-10 Fixture düzeni + altın test koşucusu**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-codecs/tests/golden.rs` · `fixtures/expected/**` · `fixtures/synthetic/**` · `fixtures/README.md` ·
    `.fabrika/raporlar/M1-10.md`
  - Okur: MIMARI §5, §6 madde 5 · QUALITY §2
  - Bağımlı: D4 (dedektör imzası D3'te)
  - Kabul: `fixtures/samples/**` ve `fixtures/synthetic/**` altındaki her dosya için dedektör + round-trip; beklenen tarif ve sonuç
    `fixtures/expected/<yol>.toml`'da · beklenti yoksa test nasıl üretileceğini yazarak düşer · `SAVEPRISM_BLESS=1` beklentileri yazar ·
    sentetik düz JSON örnekleri (sıkışık, 2 boşluk, CRLF)
  - Doğrulama: CEO `kontrol.sh rust` · yeni dosya ekleyince test kendiliğinden koşuyor
  - Denetim: dalga-şefi denetçisi (1 YUKSEK + 1 ORTA düzeltildi) · Kanıt: beklenti yokken 4 dosyada anlaşılır mesajla düştü, `SAVEPRISM_BLESS=1` sonrası geçti; CEO beklentileri inceledi: 3 exact, tabs.json semantic (tek satırlık iç dizi = karışık biçim, doğru sonuç) (2026-10-21)

- [x] **[ben] Gerçek RPG Maker MV kaydı** → kullanıcı (2026-10-22): "hiç bir oyunum yok, test için bir yerden veri çekebilir miyiz?" →
  yerine `fixtures/synthetic/rpgmaker-mv/` (orijinal `lz-string` 1.5.0 ile, kullanıcı onayıyla indirildi; kaynak `fixtures/README.md`)

---

## M2 — İlk motorlar

- **Talep:** ROADMAP M2 (HANDOFF-20261022-1 adım 3–4).
- **KAPI-1 (kullanıcı, 2026-10-22):** bağımlılıklar "Evet, ekle" · plan (D6 + D7) "Her şeyde yetkin var, overengineering yapma ve bilgisayarımı
  bozma :D" · başarısızlıkta "Güçlendir, olmazsa ayır" (bir kez güçlendir, en çok opus; olmazsa `[?]`, diğerlerine devam).
- **Bağımlılıklar (onaylı, CEO ekledi 2026-10-22):** `uesave` 0.7 (MIT, GVAS) · `aes` 0.9, `cbc` 0.2, `pbkdf2` 0.13, `sha1` 0.11
  (MIT OR Apache-2.0, ES3 şifresi) · `serde_json` 1.0 + `float_roundtrip` (MIT OR Apache-2.0; Tauri üzerinden zaten kilit dosyasında,
  yeni indirme yok; uesave'in kendi test ettiği JSON yolu, en az kod; CEO kararı, kullanıcının "her şeyde yetkin var" yetkisiyle).
  Listede olmayan paket eklenmez.
- **Gerçek kayıt yok** (kullanıcının oyunu yok, 2026-10-22). Örnekler bağımsız araçla üretilir: MZ → Python `zlib` (pako zlib'in birebir
  portu), ES3 şifreli → .NET `Rfc2898DeriveBytes` + `Aes` (ES3'ün kullandığı sınıflar, PowerShell). GVAS → yalnız `uesave` ile (bağımsız
  üretici yok; bilinen sınır). ROADMAP demosundaki "gerçek kayıt" kullanıcıya kalır (M2-7).
- **Kapsam dışı:** ad sözlüğünün arayüzü (M3), `PlM`/Oodle (Park), ES3 parola kasası (`saveprism-keys`, M4), MZ dışı RPG Maker sürümleri.
- **Kayıt düzeni:** her format `crates/saveprism-codecs/src/<format>/mod.rs` içinde `pub fn register(reg: &mut Registry)` verir;
  `src/lib.rs`'e yalnız M2-1 yazar (üç modülü birden bağlar).

### D6 — RPG Maker, ES3 düz, GVAS, açık bulgular · KAPI-1: onaylandı 2026-10-22 · Bağımlı: D5 doğrulandı

- [x] **M2-1 RPG Maker MV/MZ**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-codecs/src/rpgmaker/**` · `crates/saveprism-codecs/src/lib.rs` · `fixtures/expected/rpgmaker-mz/**` ·
    `.fabrika/raporlar/M2-1.md` (`fixtures/synthetic/rpgmaker-mz/**` ikili; CEO Python betiğiyle üretir)
  - Okur: research/02 "RPG Maker MV / MZ" · MIMARI §3.1, §3.3, §3.11 · `crates/saveprism-codecs/src/{json,layers}/mod.rs`
  - Bağımlı: D5
  - Kabul: MV `.rpgsave` = lz-string base64 + JSON (mevcut katmanlar; tarif + uzantı ipucu) · MZ `.rmmzsave` = pako `to:"string"`
    ikili metninin UTF-8 yazımı → zlib (seviye 1) → JSON; ikili metin için yeni katman `encoding.js-binary-string` (rpgmaker içinde) ·
    JsonEx `@` ile başlayan anahtarlar (`@`, `@c`, `@r`, `@a`) korunur ve düğüm ipucu `JsonEx:meta` alır · `data/System.json`'dan anahtar/değişken adları,
    `Actors.json`/`Items.json`'dan adlar okuyan `NameDictionary` (yalnız okuma; arayüz M3) · `lib.rs` rpgmaker + es3 + gvas `register`
    çağrılarını ekler · sentetik MZ örneği CEO'nun Python betiğiyle üretilir, beklenti `SAVEPRISM_BLESS=1`
  - Doğrulama: CEO `kontrol.sh rust` · MV 4 örnek + MZ örnekleri decode → encode birebir
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok; ORTA: `format.rpgmaker-json` hiç seçilmiyordu → düzeltme turu, sniff işaretle 0.95) ·
    Kanıt: `kontrol.sh rust` 3/3, 284 test (2026-10-22) · altın: MV `file1` → `format.rpgmaker-json` exact; MZ (Python zlib ile bağımsız
    üretilmiş 4 dosya) `encoding.js-binary-string > compression.zlib > …` semantic; CLI decode → encode sonrası iç JSON birebir (zlib
    baytları miniz_oxide'dan farklı) · KAPI-2: kullanıcı raporu kabul etti (2026-10-22: "Raporu okudum, itirazım yok"; oyun yok)

- [x] **M2-2 ES3 düz**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-codecs/src/es3/**` · `fixtures/synthetic/es3/**` · `fixtures/expected/es3/**` · `.fabrika/raporlar/M2-2.md`
  - Okur: research/02 "Unity — Easy Save 3" · MIMARI §3.2, §3.3 · `crates/saveprism-codecs/src/json/mod.rs` · `crates/saveprism-core/src/doc.rs`
  - Bağımlı: D5
  - Kabul: `format.es3` = JSON codec üzerine ince kat; `__type` değeri kardeş düğümlerin ipucuna `ES3:<tip>` olarak geçer, kendisi korunur ·
    bayt-bayt round-trip · sniff: `{` + `__type` ya da `.es3` uzantısı · sentetik 2 düz örnek (metin, ajan yazar)
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok) · Kanıt: `kontrol.sh rust` 3/3 (2026-10-22) · altın: `settings.es3`, `progress.es3` →
    `format.es3` exact · CLI `decode` henüz yalnız `format.json` kabul ediyor → M2-4'e eklendi · KAPI-2: rapor kabul (kullanıcı 2026-10-22)

- [x] **M2-5 Unreal GVAS (uesave)**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 1/3 (1: GitHub fuzz `detect` kırmızı — uesave paniği; sürüm profili `panic = "abort"` olduğundan `catch_unwind` sürümde çalışmıyordu → M2-5b: `unwind` + fuzz GVAS'ı atlar; release CLI fuzz girdisinde `SP-DET-001` çıkış 1, çökme yok, 2026-10-22)
  - Sahip: `crates/saveprism-codecs/src/gvas/**` · `crates/saveprism-codecs/tests/gvas.rs` · `.fabrika/raporlar/M2-5.md`
  - Okur: research/02 "Unreal — GVAS" · MIMARI §3.2–3.4 · `uesave` 0.7 genel API'si (docs.rs)
  - Bağımlı: D5
  - Kabul: `format.gvas` (`GVAS` sihirli baytı) · uesave `Save` → `serde_json::to_string` → mevcut JSON codec → SaveDoc; geri:
    JSON yazıcı → `serde_json::from_str::<Save>` → `Save::write` · tip ipuçları (`uesave::Types`) `Params`'ta (`types` tablosu) ·
    round-trip birebir değilse koruma "farklı" der (düzenleme kapalı) ·
    testler uesave ile kurulan küçük kayıtlarla (ikili dosya yazılmaz) · sınırlar: girdi boyutu ve düğüm sayısı `Limits`'ten
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok; ORTA `GVAS:<tip>` ipucu yok: çekirdekte ipucu yazma API'si gerekir, bilinçli bırakıldı;
    DUSUK: uesave uzunluk alanıyla büyük bellek isteyebilir, `catch_unwind` OOM'u yakalamaz → GVAS fuzz hedefi Park'ta) · Kanıt:
    `kontrol.sh rust` 3/3, `tests/gvas.rs` 7 test (2026-10-22) · **gerçek oyun kaydı:** uesave paketindeki Deep Rock Galactic
    `drg-save-test.sav` (565 992 bayt, repoya konmadı) → `saveprism inspect` → `format.gvas`, round trip `exact (editable: yes)` · KAPI-2: rapor kabul (kullanıcı 2026-10-22)

- [x] **FIX-1 Açık ORTA bulgular (D5b)**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-cli/src/lib.rs` · `crates/saveprism-core/src/io.rs` · `crates/saveprism-core/tests/io.rs` · `.fabrika/raporlar/FIX-1.md`
  - Okur: `.fabrika/raporlar/D5b-denetim.md` (iki ORTA satırı)
  - Bağımlı: D5
  - Kabul: CLI doğrulama kapanı yeni hedefte `SP-IO-004` yerine doğru kodu döndürür · Unix'te değiştirilen dosyanın izinleri korunur
    (`#[cfg(unix)]`, Windows'ta değişiklik yok) · ilgili testler
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (karşılandı: kapan `CORE_EDIT_FAILED`, `cfg(unix)` izin kopyası) · Kanıt: `kontrol.sh rust` 3/3 Windows'ta
    (2026-10-22); unix testi Linux/macOS CI'da koşacak (sonraki push)

### D7 — ES3 şifre, parola deneme, Palworld, hız · KAPI-1: onaylandı 2026-10-22 · Bağımlı: D6 doğrulandı

- [x] **M2-3 ES3 AES + GZip**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 1/3 (1: düzeltme turu sonrası test derlenmedi, E0716; CEO kararı: effort high, 2026-10-22)
  - Sahip: `crates/saveprism-codecs/src/es3/**` · `crates/saveprism-codecs/tests/es3_aes.rs` · `.fabrika/raporlar/M2-3.md`
    (örnekler `fixtures/keyed/es3-aes/`, CEO üretti; altın test bu klasörü taramaz çünkü parola ister)
  - Okur: research/02 "Unity — Easy Save 3" · MIMARI §3.3, §3.9
  - Bağımlı: M2-2
  - Kabul: katman `crypto.es3-aes`: `tuz[16] ‖ AES-128-CBC(PBKDF2-HMAC-SHA1(parola, tuz, 100, 16), IV = tuz, PKCS7)`; parola
    `DecodeCtx`/`Params` üzerinden (dosyaya yazılmaz) · encode aynı tuzla birebir · ardından isteğe bağlı GZip (mevcut katman) · yanlış
    parola → net SP hatası (dolgu/JSON sezgisi) · sentetik örnekler CEO'nun .NET betiğiyle
  - Doğrulama: CEO `kontrol.sh rust` · .NET ile üretilen örnek decode → encode birebir
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok; parola tarife/Params'a/log'a sızmıyor) + 3 düzeltme turu (clippy, test katılığı, E0716,
    gizli kalmış 2 `unwrap`) · Kanıt: `kontrol.sh rust` 3/3, 301 test, cargo deny ok (2026-10-22) · örnekler .NET `Rfc2898DeriveBytes` +
    `Aes` ile üretildi, Python `cryptography` ile çapraz çözüldü · `settings.es3` decode → encode birebir; GZip'li `progress-gzip.es3` düz
    metni birebir, yeniden kodlama `semantic` (ES3 `" : "` biçimi, Park) · CLI'de şifreli dosyayı parolayla açma yok (Park) · KAPI-2: rapor kabul (kullanıcı 2026-10-22)

- [x] **M2-4 `saveprism keys try`**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-cli/**` · `.fabrika/raporlar/M2-4.md`
  - Okur: MIMARI §3.9 · M2-3'ün katman API'si
  - Bağımlı: M2-3
  - Kabul: `saveprism keys try <dosya> --candidates <liste.txt>` her adayı dener, tutanı yazar; hiçbiri tutmazsa çıkış 1 · aday sayısı
    sınırlı · test: 3 adaydan doğrusu bulunuyor · `decode`/`encode` JSON ağaçlı yeni codec'leri kabul eder (D6 doğrulamasında görüldü:
    yalnız `format.json` geçiyordu)
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (ORTA: `SP-CLI` MAP.md'de yoktu → CEO ekledi) + düzeltme turu (doğrulama JSON görünümünü karşılaştırıyor,
    kullanılmayan import) · Kanıt: `kontrol.sh rust` 3/3 (2026-10-22) · elle: `keys try` 3 adaydan `found: synthetic-test-key (line 3)`
    çıkış 0, tutmayan liste `SP-CLI-002` çıkış 1 · CLI decode → encode birebir: `progress.es3` ve gerçek `drg-save-test.sav` (GVAS) ·
    düzenleme: `coins` 250 → 251, yalnız o sayı değişti, `inspect` → exact · KAPI-2: rapor kabul (kullanıcı 2026-10-22)

- [x] **M2-6 Palworld `PlZ` kabuğu**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-codecs/src/palworld/**` · `crates/saveprism-codecs/src/lib.rs` · `crates/saveprism-codecs/tests/palworld.rs` ·
    `.fabrika/raporlar/M2-6.md`
  - Okur: research/02 "Unreal — GVAS" ve Palworld notu · MIMARI §3.3
  - Bağımlı: M2-5
  - Kabul: katman `container.palworld-plz`: başlık (açık boy, sıkışık boy, `PlZ`, tür baytı) + zlib (tür 0x32 = iki kat) · `PlM` → net
    "desteklenmiyor" hatası · test: uesave ile kurulan GVAS → PlZ kabuğu → decode → encode birebir · `lib.rs`'e palworld `register`
    çağrısı (D7'de `lib.rs`'in tek sahibi)
  - Doğrulama: CEO `kontrol.sh rust`
  - Denetim: dalga denetçisi (KRITIK/YUKSEK yok; boy alanı kuralı kartla aynı) + düzeltme turu (yardımcılardaki `unwrap`) · Kanıt:
    `kontrol.sh rust` 3/3, `tests/palworld.rs` 6 test (0x31, 0x32, `PlM`, bozuk boy) (2026-10-22) · gerçek Palworld kaydı yok: testler
    uesave ile kurulan GVAS'tan · KAPI-2: rapor kabul (kullanıcı 2026-10-22)

- [x] **M2-8 Hız ölçümü**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `crates/saveprism-codecs/tests/bench_gvas.rs` · `.fabrika/raporlar/M2-8.md`
  - Okur: M2-5'in test yardımcıları
  - Bağımlı: M2-5
  - Kabul: `#[ignore]` test: uesave ile ≥50 MB GVAS kurar, decode süresini yazdırır (yeni bağımlılık yok) · CEO `--release` ile koşup
    sonucu NOW.md'ye yazar
  - Doğrulama: CEO `cargo test --release -- --ignored bench_gvas`
  - Denetim: yok (küçük iş) · Kanıt: `cargo test -p saveprism-codecs --release --test bench_gvas -- --ignored --nocapture` (2026-10-22):
    52,00 MB girdi · uesave okuma 254 ms · `format.gvas` decode (SaveDoc'a) 391 ms · encode 103 ms · birebir · not: dosya tek büyük metin
    alanı; çok sayıda küçük alanlı gerçek kayıtta süre farklı olabilir

- [?] **M2-7 [ben] Gerçek kayıtlar** → kullanıcının oyunu yok (2026-10-22). Sahip olduğu bir RPG Maker MZ, Easy Save 3 (Unity) ya da
  Unreal oyunu olursa `fixtures/samples/<motor>/` altına 2 kayıt koyar; altın test kendiliğinden koşar.

---

## v0.1 kısa yol (Windows) — D8
KAPI-1: kullanıcı kısa yolu onayladı 2026-10-22 ("bizim kullanılabilir sürmü çıkarmamız lazımdı?"); ekran tasarımı K-2'de ayrıca sorulur.

- [~] **K-1 Tauri komutları** (M3-1 sade)
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `apps/desktop/src-tauri/**` · `apps/desktop/src/api/**` · `Cargo.lock` · `.fabrika/raporlar/K-1.md`
  - Okur: MIMARI §3.4–3.6 · `saveprism-cli/src/lib.rs` · çekirdek genel API
  - Bağımlı: yok
  - Kabul: `open_file`, `get_children`, `search`, `edit_value`, `pending_changes`/`discard_changes`, `save` (yedekli, atomik, yeniden
    açıp doğrulayan; varsayılan "farklı kaydet") · hatalar `{code, message, hint}` · `src/api/commands.ts` tipli sarmalayıcılar ·
    yeni dış bağımlılık yok
  - Doğrulama: CEO `kontrol.sh rust` + `pnpm -C apps/desktop typecheck`
  - Denetim: denetci (KRITIK/YUKSEK yok; `.fabrika/raporlar/D8-denetim.md`) · Kanıt: `kontrol.sh rust` 3/3, 312 test (11 yeni masaüstü
    testi), `pnpm -C apps/desktop typecheck` çıkış 0 (2026-10-22) · kullanıcı denemesi K-4'te

- [~] **K-2 Ekran tasarımı** (M3-2) 👤
  - Motor: CEO + v0 (kullanıcı bir kez giriş yapar) · Deneme: 0/3
  - Sahip: yok (kod dosyası yok; ekran görüntüleri `.fabrika/raporlar/K-2/`)
  - Bağımlı: yok
  - Kabul: saveeditonline.com sadeliğinde tek ekran, A/B; kullanıcı birini seçer (kod/metin kopyalanmaz)
  - Doğrulama: kullanıcının seçimi · Kanıt: v0 sohbeti "SavePrism desktop app mockup" (m1ypqYGp7Yh), ekran görüntüleri
    `.fabrika/raporlar/K-2/E1…E5`; kullanıcı yönü kabul etti ("Genel tasarımla alakalı değişikliklere sonra devam ederiz … şimdilik devam et
    her şeye", 2026-10-22); kalan 6 düzeltme K-3b'de · v0 ücretsiz kredisi bitti

## D9 — tek ekran (K-3) + gerçek kayıtlar
KAPI-1: kullanıcı "şimdilik devam et her şeye" (2026-10-22). Recent files varsayılan gizli (kullanıcı: "istenmediğinde gözükmesine gerek yok").

- [~] **K-3a Oyun görünümü arka ucu** (RPG Maker bölümleri, veri klasöründen adlar, alan sınırları)
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `apps/desktop/src-tauri/**` · `apps/desktop/src/api/**` · `Cargo.lock` · `.fabrika/raporlar/K-3a.md`
  - Bağımlı: K-1
  - Kabul: `game_view`, `field_info`, sınırlı `edit_value`; sözleşme kartta (K-3b buna göre yazar)
  - Doğrulama: CEO `kontrol.sh rust` + typecheck · Denetim: denetci (KRITIK/YUKSEK yok; `.fabrika/raporlar/D9-denetim.md`) + 2 düzeltme
    turu (clippy large_enum_variant; float sınırı, 2^53 kırpma, 10 000 sınırı, take(MAX+1), göreli yol) · Kanıt: `kontrol.sh rust` 3/3,
    319 test, typecheck çıkış 0 (2026-10-22), commit `073ed58` · kullanıcı denemesi K-4'te

- [x] **K-3c Gerçek oyun kayıtları** — 65 kayıt, 16 exact · 33 semantic · 16 failed; CEO `inspect` 2 örnekte aynı sonucu aldı; yalnız yerelde
  (`fixtures/external/`, git dışı; kullanıcı: "test verilerini paylaşmaya gerek yok githubla"); ayrıntı HANDOFF-20261022-4 "K-3c sonucu"
  - Motor: claude/general-purpose·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `fixtures/external/**` · `.gitignore` · `.fabrika/raporlar/K-3c.md`
  - Bağımlı: yok
  - Kabul: ≥6 gerçek dosya, ≥3 motor, kaynak + lisans + sha256 + `inspect` sonucu `fixtures/external/SOURCES.md`'de
  - Doğrulama: CEO `inspect` örnek tekrarı · Denetim: yok · Kanıt: —

- [~] **K-3b1 Uygulama kabuğu** (başlık, başlangıç ekranı, aç/kaydet akışı, ayarlar penceresi, tema, desen) — kart `.fabrika/gorevler/K-3b1.md`
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3 · Bağımlı: K-3a + dialog eklentisi kararı (kullanıcı)
  - Sahip: kartta (`apps/desktop/src/**` hariç `src/editor/**`, `src/api/**`; `common.json`; `src-tauri` yalnız eklenti + `export_json`)
- [~] **K-3b2 Düzenleyici görünümleri** (RPG Maker bölümleri, Values listesi, alan düzenleyici) — kart `.fabrika/gorevler/K-3b2.md`
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3 · Bağımlı: K-3a
  - Sahip: `apps/desktop/src/editor/**` · `apps/desktop/locales/*/editor.json`
- K-3b kanıtı (ikisi): web 3/3 (60 test), rust 3/3 (322 test), denetim KRITIK/YUKSEK yok + düzeltme turu (`D9-denetim-K3b.md`),
  commit `5922b85` · **göz kontrolü yapılmadı** (computer-use erişimi reddedildi, tarayıcı paneli localhost'a bağlanamadı) → K-4'te kullanıcıyla

---

## Kilitli kaynaklar
| Kaynak | Şu anki sahip |
|---|---|
| fixtures/samples/** | ben (gerçek kayıt örneklerini ben koyarım) |
| LICENSE | ben |
| .claude/settings*.json, .claude/hooks/, .claude/araclar/ | ben |
