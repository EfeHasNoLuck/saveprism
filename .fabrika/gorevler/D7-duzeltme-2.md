# D7 düzeltme turu 2 (M2-3: Deneme 1/3 → effort high; M2-4, M2-6: ilk kez görülen lint, deneme sayılmaz)

- **Motor:** claude/kodcu·sonnet · **Effort:** high
- **Bash'in yok, derleyemezsin.** CEO `cargo clippy --workspace --all-targets --keep-going -- -D warnings` koştu; kalan hataların
  tam listesi aşağıda. Yalnız bunları düzelt, testlerin anlamını değiştirme, başka bir şeye dokunma.

## Sahip (yalnız bunlara yaz)
`crates/saveprism-codecs/tests/es3_aes.rs` · `crates/saveprism-codecs/tests/palworld.rs` · `crates/saveprism-cli/src/lib.rs` ·
`.fabrika/raporlar/D7-duzeltme-2.md`

## Hatalar (derleyici çıktısı)
1. `tests/es3_aes.rs:89` — `error[E0716]: temporary value dropped while borrowed`:
   `let mut ctx = DecodeCtx::new(&Limits::default()).with_step(..);` → `Limits::default()` geçici değer; önce
   `let limits = Limits::default();` ile bağla, sonra `DecodeCtx::new(&limits)`. Dosyada aynı desen başka yerde varsa onu da düzelt.
2. `tests/palworld.rs:9` — kullanılmayan import `Codec` → kaldır.
3. `tests/palworld.rs:44, 45, 51, 54` — `#[test]` olmayan yardımcı fonksiyonlarda `unwrap()` (workspace lint `unwrap_used` reddediyor).
   Yardımcıları `-> Result<Vec<u8>, Box<dyn std::error::Error>>` (ya da `std::io::Result`) döndürecek şekilde yaz ve `?` kullan;
   `#[test]` fonksiyonları yardımcının sonucunu kendi içlerinde `.unwrap()` ile açabilir (test fonksiyonunda izinli; bkz. `clippy.toml`).
   Ya da `u32::try_from(..)` için panik olmayan bir yol. Dosyadaki diğer yardımcıları da aynı gözle tara.
4. `crates/saveprism-cli/src/lib.rs:18` — kullanılmayan import `Layer` → kaldır.

## Kabul
CEO: `cargo clippy --workspace --all-targets -- -D warnings` temiz ve `cargo test --workspace` hepsi geçiyor. Rapor ≤4 satır.
