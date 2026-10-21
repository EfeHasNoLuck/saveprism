# NOW — Şu an neredeyiz?

> Her oturumda ilk okunan dosya. Tek ekran. Her görev sonunda güncellenir.

**Faz:** M0 öncesi — araştırma ve plan tamam (2026-10-21)

**Son kazanım:** Pazar taraması bitti → tüm motorları kapsayan, yerel, açık kaynak bir masaüstü editör yok; inşa kararı ve tam yol haritası hazır.

**Sıradaki tek adım:** Aşağıdaki 5 kararı ver. ⏱ ≈10 dk

---

## Bekleyen kararlar (M0-1)

1. **Proje adı** — kod adı SaveLens; değiştirmek ister misin? ([ADR-011](docs/architecture/kararlar.md))
2. **Doküman dili** — öneri: iç plan TR, public yüzey EN + TR ([ADR-008](docs/architecture/kararlar.md))
3. **RPGSaveEditor ile iş birliği** — mesajı sen mi göndereceksin? ([taslak](docs/research/outreach-rpgsaveeditor.md))
4. **Çekirdek Rust** — Rust araç zincirini kurmama onay ([ADR-002](docs/architecture/kararlar.md))
5. **Skill/MCP kurulumu** — rea + 4 çekirdek skill, proje kapsamında ([liste](docs/research/04-skills-ve-adhd-is-akisi.md))

---

## Durum özeti

| Alan | Durum |
|---|---|
| Araştırma | ✅ [01 Pazar](docs/research/01-pazar-taramasi.md) · [02 Formatlar](docs/research/02-format-ve-kutuphane-atlasi.md) · [03 rea](docs/research/03-rea-oyun-kitabi.md) · [04 Skills/ADHD](docs/research/04-skills-ve-adhd-is-akisi.md) |
| Mimari | ✅ [MIMARI.md](docs/architecture/MIMARI.md) · [kararlar.md](docs/architecture/kararlar.md) |
| Yol haritası | ✅ [ROADMAP.md](docs/ROADMAP.md) |
| Kod | ⏳ M0-5'te başlar |
| GitHub | ⏳ Yerel repo; public'e alma M5-5'te, senin onayınla |

---

## Park alanı (şimdi değil)

- Web (WASM) sürümü
- MCP sunucusu
- Kod imzalama: SignPath başvurusu, Apple notarization
- WASM codec eklentileri (Extism/wasmtime)
- Palworld `PlM` (Oodle) desteği
