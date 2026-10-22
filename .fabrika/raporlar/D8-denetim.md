# D8 denetimi (K-1) — denetci·sonnet, 2026-10-22

Denetçinin yazma aracı yok; bulguları CEO buraya aktardı.

- KRITIK: yok · YUKSEK: yok
- ORTA `session/mod.rs:891`: `verified` düzenleme yoksa yalnız yeniden çözmeye dayanıyor. → K-3'te ekran "doğrulandı" demeden önce bu
  durum yeterli (çekirdek `safe_write` geri okuyup bayt karşılaştırıyor, io.rs:97-116). Açık bırakıldı.
- ORTA `session/mod.rs:836`: "hedef var mı" denetimi ile yazma arasında yarış; yedek her durumda alınıyor, veri kaybı yok. Açık bırakıldı.
- ORTA `docs/MAP.md:47`: SP-APP-002…009 eksikti → CEO ekledi.
- ORTA parola sızıntısı: ES3 hata metinlerinde parola yok (CEO: `grep format!/message … pass|key crates/saveprism-codecs/src/es3/aes.rs` boş).
- DUSUK: kullanılmayan `INTERNAL`/`ApiError::internal()`; zehirli kilit `into_inner` ile yutuluyor. Açık bırakıldı.
- Kabul edilenler: save çekirdek `safe_write` üzerinden (yedek + atomik + geri okuma + geri alma); farklı kaydet var olan dosyaya
  `overwrite` olmadan yazmıyor (test var); üretim kodunda unwrap/expect yok; TS tipleri Rust yapılarıyla eşleşiyor; yeni dış bağımlılık yok.
