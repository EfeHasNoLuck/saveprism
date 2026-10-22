# FIX-1 — D5b'den kalan iki ORTA bulgu

- **Dalga:** D6 · **Motor:** claude/kodcu·haiku · **Effort:** medium · **Deneme:** 0/3
- **Amaç:** denetimde açık kalan iki küçük hatayı kapatmak. Kod, yorum İngilizce. Başka bir şeye dokunma.
- **Bash'in yok, derleyemezsin.** CEO derler; hataları düzeltme turunda verir.

## Okur
`.fabrika/raporlar/D5b-denetim.md` (ilk iki ORTA satırı) · `crates/saveprism-cli/src/lib.rs` · `crates/saveprism-core/src/io.rs` ·
`crates/saveprism-core/tests/io.rs` · `crates/saveprism-core/src/error.rs` (kod listesi)

## Sahip (yalnız bunlara yaz)
`crates/saveprism-cli/src/lib.rs` · `crates/saveprism-core/src/io.rs` · `crates/saveprism-core/tests/io.rs` · `.fabrika/raporlar/FIX-1.md`

## Dokunma
`error.rs` (yeni kod gerekiyorsa mevcut bir `CORE_*` kodunu kullan; yeni kod gerekirse raporda öner, ekleme), diğer her dosya.

## Yapılacaklar
1. **CLI doğrulama kapanı** (`cli/src/lib.rs` ~327–331 ve ~414–417): yazım sonrası doğrulama başarısız olduğunda `IO_VERIFY_RESTORED`
   (SP-IO-004) dönüyor; yeni hedefte geri yüklenecek bir şey yok, kod yanıltıcı. Kapanın kendi hatası için uygun mevcut `CORE_*` kodunu
   döndür (`safe_write` geri yükleme yaptıysa o katman zaten SP-IO-004 verir; ikisini karıştırma).
2. **Unix izinleri** (`core/src/io.rs` ~284–296): `tempfile` geçici dosyayı 0600 açar; `persist` sonrası eski dosyanın izni kaybolur.
   Var olan hedefin izinlerini persist'ten önce geçici dosyaya kopyala (`std::fs::metadata(..).permissions()` → `set_permissions`),
   yalnız `#[cfg(unix)]`. Windows yolu değişmez. Sahip/symlink korunmaz: bunu fonksiyonun doc yorumuna bir satır olarak yaz.
3. Testler: CLI kapanı için doğru kod (mevcut test desenine uy) · `#[cfg(unix)]` test: 0644 dosyanın üstüne yazınca 0644 kalıyor.

## Kabul
CEO: `cargo fmt --all` + `bash .claude/araclar/kontrol.sh rust` çıkış 0 (Windows'ta unix testi derlenmez; CI'da Linux/macOS'ta koşar).
Rapor ≤6 satır (`.fabrika/raporlar/FIX-1.md`).
