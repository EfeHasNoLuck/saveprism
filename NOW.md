# NOW — Şu an neredeyiz?

> Her oturumda ilk okunan dosya. Tek ekran. Her görev sonunda güncellenir.

**Faz:** M1 — çekirdek + CLI, planlama (2026-10-21)

**Son kazanım:** M1 kodu tamam: güvenli yazma (yedek A), CLI (`saveprism inspect|decode|encode`) ve dedektör bellek düzeltmesi; Rust kontrolü 3/3,
sentetik kayıtta decode→encode bayt-bayt aynı (2026-10-22). Kalan: senin gerçek kayıt denemen (KAPI-2) + fuzz koşusu (push).

**Sıradaki tek adım:** gerçek RPG Maker MV kaydını `fixtures/samples/rpgmaker/` altına koy; sonra `saveprism decode k.rpgsave -o f.json` →
`saveprism encode f.json --recipe f.recipe.toml -o out.rpgsave` ve `fc /b k.rpgsave out.rpgsave` aynı çıkmalı. ⏱ ~10 dk

---

## Kararlar (M0-1)

1. ✅ **Ad:** SavePrism ([ADR-011](docs/architecture/kararlar.md)) — public öncesi (M5) ücretsiz marka araması: TÜRKPATENT · EUIPO · USPTO
2. ✅ **Dil:** sohbet TR, repo EN ([ADR-008](docs/architecture/kararlar.md)) — çeviri M0-12
3. ✅ **RPGSaveEditor:** muhtaç değiliz ama iş birliğine açığız; v0.1 sonrası not + ortak fixture/format havuzu önerisi ([ADR-010](docs/architecture/kararlar.md)) — M5-6
4. ✅ **Çekirdek dili:** Rust ([ADR-002](docs/architecture/kararlar.md))
5. ✅ **CI:** private repo [huseyincancalti/saveprism](https://github.com/huseyincancalti/saveprism) (2026-10-21). Actions dakikası biterse B: yerel `kontrol.sh`, git yerelde sürer, push sonraki ay.

> Not: Rust PATH'e eklendi ama açık uygulamalar eski PATH'i görür. Claude uygulamasını/terminali yeniden başlatınca `kontrol.sh` cargo'yu bulur.

---

## Durum özeti

| Alan | Durum |
|---|---|
| Araştırma | ✅ [01 Pazar](docs/research/01-pazar-taramasi.md) · [02 Formatlar](docs/research/02-format-ve-kutuphane-atlasi.md) · [03 rea](docs/research/03-rea-oyun-kitabi.md) · [04 Skills/ADHD](docs/research/04-skills-ve-adhd-is-akisi.md) |
| Mimari | ✅ [MIMARI.md](docs/architecture/MIMARI.md) · [kararlar.md](docs/architecture/kararlar.md) · [QUALITY.md](docs/architecture/QUALITY.md) |
| Riskler ve harita | ✅ [RISKS.md](docs/RISKS.md) (Non-Goals dahil) · [MAP.md](docs/MAP.md) |
| Yol haritası | ✅ [ROADMAP.md](docs/ROADMAP.md) |
| Çalışma düzeni | ✅ [Fabrika](docs/FABRIKA.md) (CEO + alt ajanlar, KAPI-1/KAPI-2) · TASKS.md · handoff/ · 6 skill + rea MCP |
| Kod | ⏳ M0-5'te başlar |
| GitHub | ⏳ Yerel repo; public'e alma M5-5'te, senin onayınla |

---

## Park alanı (şimdi değil)

- **Oyun vitrini** ("Hangi oyunda kullandın?" + çalışan oyunlar listesi, 18+ sansürlü) — araştırma bitti → `docs/research/06-game-showcase-feasibility.md` §8;
  kullanıcı girdisi (2026-10-21): ayarlarda 18+ anahtarı + yaş sorusu, internet yalnız kişi isterse, oyun görselleri isteniyor (yeni karar D6). M7'den önce değil.
- **Düzenleme ekranı mockup'ı (A/B)** — hazır: https://claude.ai/artifact/GKHiaZcNuLQjNXiSKrK3ou (A klasik üç panel, B odak görünümü;
  hazırlayanın önerisi: B varsayılan + A'nın ağacı/denetçisi çekmecede). Kullanıcının seçimi bekleniyor → M3-2 düzenine girdi

- Web (WASM) sürümü
- MCP sunucusu
- Kod imzalama: SignPath başvurusu, Apple notarization
- WASM codec eklentileri (Extism/wasmtime)
- Palworld `PlM` (Oodle) desteği
