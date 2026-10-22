# D7 düzeltme turu 3 raporu

- Değişen dosya: `crates/saveprism-codecs/tests/es3_aes.rs`. `fixture` artık `std::io::Result<Vec<u8>>` döner (`std::fs::read(&path)` doğrudan), `with_password` `Option<Recipe>` döner (`get_mut(0)?`). Yardımcılardaki iki `unwrap` kaldırıldı (clippy hata 1 ve 2). Çağrı yerleri `#[test]` içinde `.unwrap()` ile güncellendi. Yardımcılarda başka `unwrap`/`expect` yok, kalan tümü `#[test]` gövdelerinde.
- Doğrulama (Bash yok, çalıştırılmadı): `bash .claude/araclar/kontrol.sh rust` veya `cargo clippy --workspace --all-targets -- -D warnings` ve `cargo test --workspace`.
- Emin olunmayan: çok satırlı `pipeline::encode(...)` ifadesinin biçimi rustfmt çıktısıyla birebir aynı olmayabilir; clippy/test etkilemez.
