# D7 denetim

0 KRITIK, 0 YUKSEK, 1 ORTA, 2 DUSUK (kod okundu, derleme/test koşulmadı).

- ORTA · crates/saveprism-cli/src/lib.rs:40 · `SP-CLI` öneki (SP-CLI-001/002) docs/MAP.md hata öneki tablosunda yok. CEO ekler.
- DUSUK · crates/saveprism-cli/src/lib.rs:596 · bulunan parola stdout'a yazılıyor; kart istiyor, sızıntı değil.
- DUSUK · crates/saveprism-codecs/src/es3/aes.rs:28 · encode için çağıran `password`'ü eklemeli; CLI encode/decode'da parola yolu yok (kart kapsamı dışı).

Kontroller: parola tarif/Params/log'a sızmıyor (aes.rs:134-136, tests/es3_aes.rs:131-143); test dışı kodda unwrap/expect/panik yok; Palworld boy alanı kuralı kartla aynı (palworld/mod.rs:94-145); overengineering yok; Sahip dışı yazım yok, yeni bağımlılık yok.
