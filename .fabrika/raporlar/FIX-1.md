# FIX-1 raporu
- cli/src/lib.rs: decode ve encode doğrulama kapanları IO_VERIFY_RESTORED yerine CORE_EDIT_FAILED döner (tam eşleşen kod yok; yeni kod önerilir: SP-IO-008 "yazılan bayt beklenenle farklı"). Kapanlara yorum eklendi. Test modülü eklendi: `decode_verification_mismatch_uses_a_core_code`.
- core/src/io.rs: `copy_existing_mode` (#[cfg(unix)]) hedef varsa izin bitlerini geçici dosyaya kopyalar; `atomic_write` persist öncesi çağırır, hata temp_error ile döner. Doc yorumuna "sahip ve symlink korunmaz" satırı eklendi. Windows yolu değişmedi.
- core/tests/io.rs: `#[cfg(unix)] existing_file_keeps_its_unix_permissions` (0644 üstüne yazınca 0644 kalır).
- Doğrulama (CEO): `cargo fmt --all` ve `bash .claude/araclar/kontrol.sh rust` çıkış 0. Unix testi Linux/macOS CI'da koşar.
- Emin değilim: CORE_EDIT_FAILED kodu anlam olarak tam uymuyor; CEO yeni IO kodu isterse error.rs'e ekleme gerekir (bu görevde eklenmedi).
- Kullanıcıya kalan: IO_RESTORE_FAILED / IO_BACKUP testleri (kartta yoktu, DUSUK bulgu olarak kaldı).
