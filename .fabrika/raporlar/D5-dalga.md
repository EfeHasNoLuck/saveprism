# Dalga 5 (1. kısım) raporu

- M1-5 · yazıldı, derlenmedi · core/src/detect.rs, core/src/lib.rs (test satırı), codecs/tests/detect.rs · .fabrika/raporlar/M1-5.md
- M1-9 · yazıldı, derlenmedi · codecs/tests/limits.rs, fuzz/{Cargo.toml,.gitignore,fuzz_targets/*.rs}, .github/workflows/fuzz.yml · .fabrika/raporlar/M1-9.md
- M1-10 · yazıldı, derlenmedi · codecs/tests/golden.rs, fixtures/synthetic/json/*.json (4), fixtures/README.md · .fabrika/raporlar/M1-10.md
- Denetim: .fabrika/raporlar/D5-denetim.md. Bir düzeltme turu yapıldı (detect.rs allow, limits.rs sıkılaştırma).
- Kalan: ORTA golden beklentileri yok (bless), ORTA MAP.md eksik, DUSUK detect bellek üst sınırı.
- Kullanıcı/CEO: `cargo fmt --all`, `kontrol.sh rust`, `SAVEPRISM_BLESS=1 cargo test -p saveprism-codecs --test golden` ve .toml incelemesi, fuzz'ı nightly ile deneme.
