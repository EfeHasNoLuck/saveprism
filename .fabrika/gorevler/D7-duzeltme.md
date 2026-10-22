# D7 düzeltme turu (deneme sayılmaz)

- **Motor:** claude/kodcu·sonnet · **Effort:** medium
- **Bash'in yok, derleyemezsin.** CEO derledi ve testleri koştu; aşağıdaki üç sorun kaldı. Başka bir şeye dokunma.

## Sahip (yalnız bunlara yaz)
`crates/saveprism-codecs/src/es3/aes.rs` · `crates/saveprism-codecs/tests/es3_aes.rs` · `crates/saveprism-cli/src/lib.rs` ·
`crates/saveprism-cli/tests/cli.rs` · `.fabrika/raporlar/D7-duzeltme.md`

## 1. Clippy (`es3/aes.rs:101`)
`input.len().saturating_sub(IV_LEN) % BLOCK == 0` → `clippy::manual_is_multiple_of`. `.is_multiple_of(BLOCK)` kullan.

## 2. CLI ES3 gidiş-dönüşü (`cli/tests/cli.rs:306`, `es3_plain_file_decodes_and_encodes_byte_for_byte`)
Çıktı: `SP-CORE-012: the written file decodes to a different document`, çıkış 1.
Olası neden (kodu okuyarak doğrula): `encode`, JSON dosyasını `format.json` ile okuyor (tip ipucu yok); yazım sonrası doğrulama yazılan
dosyayı tarifle (`format.es3`) çözüyor, o da `ES3:<tip>` ipuçlarını ekliyor; belge karşılaştırması ipuçlarını da sayıyor → "farklı".
Beklenen davranış: kullanıcının düzenlediği şey JSON metnidir; doğrulama, yazılan dosyanın **JSON görünümü** girdi JSON'uyla aynı mı
diye bakmalı. Sade yol (birini seç, rapora yaz): (a) doğrulamada iki belgeyi JSON codec ile aynı biçim profiliyle metne çevirip karşılaştır;
ya da (b) çekirdekte tip ipuçlarını yok sayan bir eşitlik varsa onu kullan. `format.json` için bugünkü davranış ve testler değişmemeli.
Gerçek neden başkaysa onu düzelt ve rapora yaz.

## 3. GZip'li şifreli ES3 testi çok katı (`tests/es3_aes.rs:77`, `gzip_file_opens_and_keeps_its_content`)
Orijinal düz metin ES3'ün güzel yazımını kullanıyor (`"level" : {` ve tek satır dizi `[1,2,3,5,8]`); JSON codec'i bu karışık biçimi
yeniden üretemiyor, bu yüzden yeniden kodlanmış metin farklı. Kart bu dosya için `semantic` sonucu kabul ediyordu.
Testi şöyle değiştir: (a) çözülmüş düz metin (decode çıktısı, yeniden kodlamadan önce) orijinalle **bayt bayt** aynı; (b) koruma sonucu
`semantic` ya da `exact` (`editable = true`); (c) yeniden kodla → yeniden çöz → belge eşit. GZip'siz `settings.es3` testi birebir kalmalı.

## Kabul
CEO: `cargo clippy --workspace --all-targets -- -D warnings` temiz, `cargo test --workspace` hepsi geçiyor. Rapor ≤5 satır.
