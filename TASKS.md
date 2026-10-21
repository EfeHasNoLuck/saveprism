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

- [~] **CI-1 Belge değişikliğinde CI koşmasın**
  - Motor: claude/kodcu·haiku · Effort: medium · Deneme: 0/3
  - Sahip: `.github/workflows/ci.yml`
  - Okur: —
  - Bağımlı: KAPI-1
  - Kabul: `push` ve `pull_request` tetiklerine `paths-ignore`: `docs/**`, `**/*.md`, `handoff/**`, `.fabrika/**`; başka değişiklik yok
  - Doğrulama: CEO diff'e bakar · yalnız belge değiştiren sonraki push'ta GitHub'da CI koşusu açılmaz
  - Denetim: yok (küçük iş) · Kanıt: CEO `git diff` → yalnız `on.push` ve `on.pull_request` altına 4 satırlık `paths-ignore` eklendi,
    işler aynen (2026-10-21) · GitHub'da belge-push denemesi: bekliyor

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

- [~] **M1-9 Girdi sınırları + ilk fuzz hedefi**
  - Motor: claude/kodcu·sonnet · Effort: medium · Deneme: 0/3
  - Sahip: `fuzz/**` · `.github/workflows/fuzz.yml` · `crates/saveprism-codecs/tests/limits.rs` · `.fabrika/raporlar/M1-9.md`
  - Okur: QUALITY §2 · `crates/saveprism-core/src/limits.rs`
  - Bağımlı: D4
  - Kabul: gzip/zlib bombası, aşırı derin JSON, aşırı büyük girdi → panik yerine SP hatası · proptest: rastgele bayt her decode'a ve
    dedektöre → panik yok · `fuzz/` hedefleri (detect, json, layers) · fuzz.yml: haftalık + elle + `crates/**` push'unda, Linux, 60 sn/hedef
  - Doğrulama: CEO `kontrol.sh rust` · fuzz işi GitHub'da bir kez yeşil
  - Denetim: dalga-şefi denetçisi (1 YUKSEK + 1 ORTA düzeltildi) · Kanıt: tests/limits.rs 7 test yeşil (bombalar, derinlik, girdi, düğüm, proptest) · fuzz/ ve fuzz.yml yazıldı; yerelde nightly yok, fuzz koşusu ilk push'ta GitHub'da (2026-10-21)

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

## Kilitli kaynaklar
| Kaynak | Şu anki sahip |
|---|---|
| fixtures/samples/** | ben (gerçek kayıt örneklerini ben koyarım) |
| LICENSE | ben |
| .claude/settings*.json, .claude/hooks/, .claude/araclar/ | ben |
