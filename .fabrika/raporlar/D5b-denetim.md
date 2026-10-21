# D5 part 2 denetim (M1-8, M1-6, M1-5b)

KRITIK/YUKSEK: yok. Derleme/test çalıştırılmadı.

- ORTA crates/saveprism-cli/src/lib.rs:327-331, 414-417: doğrulama kapanları IO_VERIFY_RESTORED (SP-IO-004) döndürüyor; yeni hedefte geri yükleme yok, kod yanıltıcı. CORE_* türü kod önerildi.
- ORTA crates/saveprism-core/src/io.rs:284-296: tempfile 0600 oluşturur; Unix'te eski dosyanın izin/sahip/symlink bilgisi persist ile kaybolur (Windows'ta etkisiz). İzin kopyala ya da bilinen sınırlama yaz.
- DUSUK crates/saveprism-core/src/detect.rs:147,227: pop_best ile çıkan durum bayt toplamına girmiyor; doc notu düş.
- DUSUK crates/saveprism-codecs/tests/detect.rs:91-104: test gerçek dallanmayı zorlamıyor; bütçe kanıtı yalnız detect.rs:350 birim testinde.
- DUSUK crates/saveprism-cli/src/lib.rs:115-122: stdout yazma hatası IO_TEMP_WRITE ile bildiriliyor.
- DUSUK crates/saveprism-cli/src/lib.rs:44-47,59-61: kartta olmayan decode --backup-dir (gerekçeli).
- DUSUK crates/saveprism-core/tests/io.rs: IO_RESTORE_FAILED ve IO_BACKUP yolları için test yok.

Doğrulananlar: atomik yazma (aynı klasör temp, sync_all, persist, artık yok), yedek backup_root altında (kural 1), geri alma, budama (en yeni kalır, tanınmayan dosyaya dokunulmaz), üretim kodunda panik yok, sahiplik/bağımlılık ihlali yok.
