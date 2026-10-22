# D6 düzeltme turu (deneme sayılmaz)

- **Motor:** claude/kodcu·sonnet · **Effort:** medium
- **Bash'in yok, derleyemezsin.** CEO derledi; aşağıdaki iki sorun kaldı. Başka bir şeye dokunma.

## Sahip (yalnız bunlara yaz)
`crates/saveprism-codecs/src/rpgmaker/**` · `crates/saveprism-codecs/tests/gvas.rs` · `.fabrika/raporlar/D6-duzeltme.md`

## 1. `format.rpgmaker-json` hiç seçilmiyor (M2-1)
Kanıt: altın test MV ve MZ için tarifi `… > format.json` buluyor; `rpgmaker/mod.rs` codec'inin `sniff`'i hep NONE. Sonuç: `JsonEx:meta`
ipucu üründe hiç oluşmuyor.
Yapılacak: `sniff`, girdi JSON gibi başlıyorsa (`{` ya da `[`, baştaki boşluk atlanır) ve ilk 64 KB içinde JsonEx işareti varsa
(`"@":"Game_` ya da `"@c":` ya da `"@a":`) `format.json`'ın sniff güveninden **yüksek** bir güven döndürsün (`json/mod.rs`'teki değere bak);
yoksa NONE. Böylece dedektör MV/MZ kayıtlarında kendiliğinden `format.rpgmaker-json` seçer. Uzantıya bakma.
Testlerde (`rpgmaker/tests.rs`) MV ve MZ örnek girdisinde en iyi tarifin son adımı `format.rpgmaker-json` olmalı; JsonEx işareti
olmayan düz JSON'da `format.json` kalmalı. `global.rmmzsave` gibi dizi kökü (`[null,{…,"@":"Object"}]`) de işaret içeriyorsa seçilir.

## 2. Clippy hatası (M2-5)
`crates/saveprism-codecs/tests/gvas.rs:9`: `i32::try_from(text.len() + 1).unwrap()` → workspace lint'i `unwrap_used` reddediyor
(yardımcı fonksiyon `#[test]` değil). Yardımcıyı `Result` döndürecek ya da `unwrap` gerektirmeyecek şekilde yaz (ör. sabit küçük
metinler için `u8`/`i32` dönüşümünü `as` yerine `try_from(..).unwrap_or(i32::MAX)` gibi panik olmayan bir yola çek; testin anlamını
değiştirme). Dosyada başka `unwrap`/`expect` varsa ve `#[test]` fonksiyonunun dışındaysa onları da düzelt.

## Kabul
CEO: `cargo clippy --workspace --all-targets -- -D warnings` temiz, `kontrol.sh rust` 3/3 (MV/MZ beklentilerini CEO yeniden üretir).
Rapor ≤5 satır.
