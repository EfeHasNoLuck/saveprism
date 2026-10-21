# NOW — Şu an neredeyiz?

> Her oturumda ilk okunan dosya. Tek ekran. Her görev sonunda güncellenir.

**Faz:** M0 — temel kurulum (2026-10-21)

**Son kazanım:** Ad **Save-R** oldu; dil kuralı (sohbet TR, repo EN) ve bağımsız yol (RPGSaveEditor'e not v0.1 sonrası) kayda geçti. Risk kaydı, kalite stratejisi ve kod haritası hazır.

**Sıradaki tek adım:** Çekirdek dili için A/B kararını ver (Rust mı, TypeScript mi). ⏱ 2 dk

---

## Kararlar (M0-1)

1. ✅ **Ad:** Save-R ([ADR-011](docs/architecture/kararlar.md)) — yeniden adlandırma M0-11
2. ✅ **Dil:** sohbet TR, repo EN ([ADR-008](docs/architecture/kararlar.md)) — çeviri M0-12
3. ✅ **RPGSaveEditor:** bağımsız; eşit seviyede not v0.1 sonrası ([ADR-010](docs/architecture/kararlar.md)) — M5-6
4. ⏳ **Çekirdek dili:** A = Rust (öneri) · B = TypeScript ([ADR-002](docs/architecture/kararlar.md))

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

- Web (WASM) sürümü
- MCP sunucusu
- Kod imzalama: SignPath başvurusu, Apple notarization
- WASM codec eklentileri (Extism/wasmtime)
- Palworld `PlM` (Oodle) desteği
