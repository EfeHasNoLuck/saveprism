D6 düzeltme
- rpgmaker/mod.rs: `sniff` artık `{`/`[` ile başlayan ve ilk 64 KB'ta `"@":"Game_`, `"@c":` ya da `"@a":` içeren girdiye 0.95 döndürür (json en çok 0.9); yoksa NONE. Modül notu güncellendi.
- rpgmaker/tests.rs: MZ altın zinciri son adım `format.rpgmaker-json`; eski "asla seçilmez" testi sniff testine çevrildi; yeni `detector_picks_rpgmaker_json_only_with_markers` (düz JSON -> format.json).
- tests/gvas.rs: `put_string` içinde `unwrap` -> `unwrap_or(i32::MAX)`. Diğer unwrap'lar `#[test]` içinde.
- Doğrulama: `cargo clippy --workspace --all-targets -- -D warnings`, `bash .claude/araclar/kontrol.sh rust`. Derlenmedi; MV/MZ altın beklentileri (son adım) CEO tarafından yeniden üretilmeli.
