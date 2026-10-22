# D7-duzeltme raporu
1. es3/aes.rs: `% BLOCK == 0` -> `.is_multiple_of(BLOCK)`.
2. cli/src/lib.rs `encode` doğrulaması: `tree_eq` başarısızsa iki belgeyi JSON codec ile (tarifin son adım parametreleriyle) metne çevirip karşılaştırır (seçenek a). Neden: `format.es3` decode'u ES3 tip ipuçları ekliyor. `format.json` davranışı değişmedi (tree_eq önce denenir).
3. tests/es3_aes.rs gzip testi: (a) çözülmüş düz metin orijinalle bayt bayt aynı (satır sonu LF/CRLF ikisi de kabul), (b) codec encode -> decode belge eşit (guard'ı kullanmadım: guard katman encode'unu parolasız tarif parametreleriyle dener, şifreli dosyada Different verir), (c) yeniden kodla -> çöz -> belge eşit. settings.es3 testi aynen.
Emin değilim: `DecodeCtx::new(&Limits).with_step(..)` imzası guard.rs'teki kullanıma göre yazıldı; derlenmedi.
