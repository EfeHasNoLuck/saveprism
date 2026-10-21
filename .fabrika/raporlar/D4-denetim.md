# D4 denetim (derleme yapılmadı, salt okunur)

Sahiplik, kilitli kaynak, bağımlılık, unwrap/panic/indeksleme yasağı, açma sınırı: temiz.

| Derece | Yer | Bulgu | Durum |
|---|---|---|---|
| KRITIK | layers/lzstring.rs:20-21 | URI_ALPHABET 65 bayt, tip &[u8;64]; derleme hatası | Düzeltme turunda giderildi (64 simge, `$` dolgu yuvası), yeniden denetlenmedi |
| YUKSEK | json/tests.rs:175 | br"" içinde ASCII dışı; derleme hatası | Düzeltme turunda giderildi, yeniden denetlenmedi |
| YUKSEK | core/src/lib.rs:64-66 | Bayat test: check_round_trip artık CORE_NOT_IMPLEMENTED dönmüyor (CORE_INVALID_RECIPE). lib.rs hiçbir kartın Sahip listesinde değil | AÇIK: CEO düzeltmeli |
| ORTA | layers/gzip.rs:334-341, zlib.rs:183-192 | level_found == level kararsız olabilir | Assert kaldırıldı (exact + bayt eşitliği kaldı) |
| DUSUK | layers/deflate.rs:67-74 | Seviye bulma 10 sıkıştırma, guard ile iki kat | Kabul |
| DUSUK | layers/lzstring.rs:140-206 | Sözlük için ayrı üst sınır yok | Kabul |
| DUSUK | layers/base64.rs:172 | decode bütçeyi kullanmıyor (çıktı girdiden küçük) | Kabul |
| DUSUK | json/write.rs:247 | Üs eşiği 1e-7 (JS 1e-6); kart öyle istiyor | Kabul |
| DUSUK | json/parse.rs:266-272 | Eşsiz vekilli nesne anahtarı SP-JSON-005 ile reddedilir | Belgelenmiş sınırlama |
