# D6 denetim (denetci, kod okuyarak; derleme yok)

KRITIK: 0 · YUKSEK: 0

- ORTA gvas/mod.rs:14-16,140-142: `GVAS:save` kök ipucu verilmemiş. Gerekçe es3/plain.rs:63-117 ile çelişiyor (ctx.new_doc + push ile yeniden kurma mümkün). Kart "yapamıyorsan verme, raporda yaz" dediği için kabul edilebilir.
- ORTA docs/MAP.md:44: `SP-RPGM` yazıyor, kod `SP-RPG-001..004`. rpgmaker, es3, gvas modülleri ve SP-GVAS-003 MAP.md'de yok (CEO/docs).
- ORTA rpgmaker/mod.rs:13-15,150-158: `format.rpgmaker-json` hiçbir tarifte kullanılmıyor (sniff hep NONE); dedektör format.json seçtiği için JsonEx:meta ipucu üründe oluşmaz. Bağlama CEO'ya kalıyor.
- DUSUK gvas/mod.rs:130: uesave uzunluk alanlı bellek ayırma sınırsız olabilir; catch_unwind OOM yakalamaz.
- DUSUK es3/mod.rs:35, rpgmaker/mod.rs:59: codec görünürlüğü tutarsız (pub(crate) / pub).
- DUSUK rpgmaker/tests.rs:9: import sırası rustfmt'e uymuyor.

Özel kontroller: register imzaları tutarlı (rpgmaker/mod.rs:52, es3/mod.rs:30, gvas/mod.rs:42), lib.rs:8-20 bağlıyor. Overengineering yok. Sahip dışı yazım yok. FIX-1 karşılandı (CORE_EDIT_FAILED; cfg(unix) izin kopyası).
