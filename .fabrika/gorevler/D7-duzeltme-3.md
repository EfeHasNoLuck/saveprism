# D7 düzeltme turu 3 (yeni görünen lint; önceki derleme hatası gizliyordu)

- **Motor:** claude/kodcu·haiku · **Effort:** medium
- **Bash'in yok.** Testlerin hepsi geçiyor (301); yalnız clippy kaldı. Başka bir şeye dokunma.

## Sahip (yalnız bunlara yaz)
`crates/saveprism-codecs/tests/es3_aes.rs` · `.fabrika/raporlar/D7-duzeltme-3.md`

## Hatalar (`cargo clippy --workspace --all-targets -- -D warnings`)
1. `tests/es3_aes.rs:16` — `std::fs::read(&path).unwrap()` `#[test]` olmayan yardımcıda.
2. `tests/es3_aes.rs:46` — `recipe.steps.get_mut(0).unwrap()` `#[test]` olmayan yardımcıda.
Yardımcıları `Result`/`Option` döndürecek şekilde yaz (`?`), açmayı `#[test]` fonksiyonlarında `.unwrap()` ile yap (orada izinli).
`tests/palworld.rs`'teki yeni desen örnek olsun. Dosyada `#[test]` dışında başka `unwrap`/`expect` kalmasın.

## Kabul
CEO: `cargo clippy --workspace --all-targets -- -D warnings` temiz, `cargo test --workspace` hepsi geçiyor. Rapor ≤3 satır.
