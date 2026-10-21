# D5 (1. kısım) denetimi

Denetçi derleyemedi; bulgular okumaya dayanır.

| Derece | Yer | Bulgu | Durum |
|---|---|---|---|
| YUKSEK | codecs/tests/detect.rs:25-38 | `.unwrap()` yardımcılarda, `allow` yok, clippy düşer | Düzeltildi (dosya başı allow) |
| ORTA | codecs/tests/limits.rs:103-111 | detect testi `CORE_NOT_IMPLEMENTED` kabul ediyordu | Düzeltildi (yalnız `CORE_LIMIT_INPUT`) |
| ORTA | codecs/tests/golden.rs, fixtures/expected | Beklenti yok; bless edilene kadar `golden_corpus` düşer | Açık (CEO bless eder) |
| ORTA | docs/MAP.md:13,42 | `fuzz/` ve `fixtures/` düzeni yok, `detect/` yolu yanlış | Açık (kart dışı, CEO) |
| DUSUK | core/src/detect.rs:57,194 | Ön cephe en kötü 16 x `max_decoded_bytes` bellek tutabilir | Açık (varsayılan sınırı kontrol et) |

Belirsiz: tests/detect.rs rastgele-bayt testi sniff davranışına bağlı. limits.rs lz-string bomba testi çalıştırılmadı.
Düzeltme turundan sonra yeniden denetim yapılmadı; düzeltmeler tek satırlıktı, CEO derlemesi doğrular.
