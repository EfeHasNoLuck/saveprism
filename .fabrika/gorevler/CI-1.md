# CI-1 — Belge değişikliğinde CI koşmasın

- **Dalga:** D3 · **Motor:** claude/kodcu·haiku · **Effort:** medium · **Deneme:** 0/3
- **Amaç:** private repoda Actions dakikası (macOS 10 kat sayılıyor) yalnız belge değişen commit'lerde harcanmasın.

## Okur
`.github/workflows/ci.yml`

## Sahip (yalnız bunlara yaz)
`.github/workflows/ci.yml`

## Dokunma
Başka her dosya. İşlerin (jobs) içeriğine dokunma.

## Yapılacak
`on.push` ve `on.pull_request` tetiklerinin ikisine de aynı listeyi ekle (mevcut `branches` gibi alanlar kalır):
```yaml
paths-ignore:
  - "docs/**"
  - "**/*.md"
  - "handoff/**"
  - ".fabrika/**"
```
Tetik şu an kısa biçimdeyse (`on: [push, pull_request]`) eşdeğer uzun biçime çevir.

## Kabul
Yalnız tetik bölümü değişti · YAML geçerli (girinti 2 boşluk) · iş tanımları aynen duruyor.

## Doğrulama
CEO `git diff` · yalnız belge değiştiren sonraki push'ta GitHub'da CI koşusu açılmıyor.

## Rapor (≤5 satır)
Değişen satırlar ve tetiğin son hali. Rapor dosyası yok; dönüş metni yeter.
