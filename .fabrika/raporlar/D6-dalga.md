# D6 dalga raporu

Hiçbiri derlenmedi/test edilmedi (kodcuların Bash'i yok). Doğrulama CEO'da.

- M2-1 · yazıldı (doğrulanmadı) · crates/saveprism-codecs/src/rpgmaker/{mod,binstr,names,tests}.rs, src/lib.rs · .fabrika/raporlar/M2-1.md
- M2-2 · yazıldı (doğrulanmadı) · crates/saveprism-codecs/src/es3/{mod,plain}.rs, fixtures/synthetic/es3/{settings,progress}.es3 · .fabrika/raporlar/M2-2.md
- M2-5 · yazıldı (doğrulanmadı) · crates/saveprism-codecs/src/gvas/mod.rs, tests/gvas.rs · .fabrika/raporlar/M2-5.md
- FIX-1 · yazıldı (doğrulanmadı) · crates/saveprism-cli/src/lib.rs, crates/saveprism-core/src/io.rs, crates/saveprism-core/tests/io.rs · .fabrika/raporlar/FIX-1.md

Denetim: .fabrika/raporlar/D6-denetim.md (KRITIK 0, YUKSEK 0, ORTA 3, DUSUK 3). Düzeltme turu gerekmedi.

CEO/kullanıcıya kalan: cargo fmt + kontrol.sh rust; MZ/ES3 fixture beklentileri `SAVEPRISM_BLESS=1` (golden test yolu fixtures/expected/synthetic/<motor>/, kartlardaki yol değil); MZ örneklerini Python zlib ile üretmek; fixtures/README.md'ye es3 kaynak notu; MAP.md düzeltmesi; drg-save-test.sav yerel denemesi; format.rpgmaker-json'ın dedektöre bağlanması; locale metinleri error.sp-gvas-001..003; GVAS:save ipucu kararı.
