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
  clsx + tailwind-merge yerine geçen paketi), `shadcn` (MIT, CSS için), `@fontsource-variable/geist` (OFL-1.1 font) → font kararı kullanıcıda
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

## Kilitli kaynaklar
| Kaynak | Şu anki sahip |
|---|---|
| fixtures/samples/** | ben (gerçek kayıt örneklerini ben koyarım) |
| LICENSE | ben |
| .claude/settings*.json, .claude/hooks/, .claude/araclar/ | ben |
