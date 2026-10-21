---
name: munazara
description: Geri dönüşü pahalı kararlarda (çekirdek mimari, format desteği kapsamı, lisans/hukuk, dağıtım) tek modelin onaylayıcılığını kırmak için çok personalı münazara kurar. Bağımsız görüş → çürütme → CEO sentezi (xhigh) → karar kullanıcının. "Münazara kur", "bu kararı münazarayla ver", "farklı bakış açılarıyla değerlendir" dendiğinde kullan.
argument-hint: "<karar sorusu>"
---
# Münazara (sade)

**Maliyet uyarısı:** yaklaşık 6–9 alt ajan çağrısı. Başlamadan kullanıcıya söyle ve onay al. Küçük, geri alınabilir kararda münazara kurma.

1. **SORU:** `.fabrika/munazara/<kisa-konu>/SORU.md` yaz: karar sorusu, 2–4 seçenek, kısıtlar (tek geliştirici, Pro kotası, MIT lisansı,
   AGENTS.md ürün kuralları), ilgili kaynak bölümleri (`docs/architecture/MIMARI.md` bölümleri, `kararlar.md` ADR'leri,
   `docs/research/` bulguları, dosya:satır). Olguları dosyalardan bul; kullanıcıya yalnız karar sor.
2. **Personalar (3–4, konuya göre seç):** Format/tersine mühendislik uzmanı · Rust/Tauri teknik lead · Kullanıcı savunucusu
   (güven, gizlilik, etik/hukuk) · Kapsam/zaman (tek geliştirici) · Açık kaynak bakımcısı (katkıcı deneyimi, sürdürülebilirlik).
   Codex ya da agy kuruluysa bir personayı dış motora ver (farklı motor = farklı kör nokta; komut `dis-motor` skill'inde, salt okuma kipinde).
3. **Görüş turu (paralel, tek mesajda):** her persona bir `munazaraci` alt ajanı. İstem:
   `Persona: <ad> — <bakış açısı>. SORU: .fabrika/munazara/<konu>/SORU.md. Tur: gorus. Yaz: .fabrika/munazara/<konu>/<persona>-gorus.md`
   Personalar birbirini görmez.
4. **Çürütme turu (paralel):** aynı personalar, `Tur: curutme`, diğerlerinin görüş dosyalarını okur, `<persona>-curutme.md` yazar.
5. **Sentez (sen, CEO, xhigh effort):** bütün dosyaları oku ve `.fabrika/munazara/<konu>/SENTEZ.md` yaz:
   seçenekler · her seçeneğin en güçlü argümanı ve çürütülemeyen riski · uzlaşılan ve uzlaşılamayan noktalar · önerin ve gerekçesi ·
   geri dönüş maliyeti · bu kararı neyin değiştireceği (ölçülecek şey: benchmark, kullanıcı testi, issue sayısı).
6. **Karar kullanıcının (KAPI):** sentezi 10 satırda özetle ve seçimi sor. Karar `docs/architecture/kararlar.md`'ye yeni ADR olarak
   tarih, kaynak ve "münazara: .fabrika/munazara/<konu>/" bağlantısıyla yazılır.

Effort: personalar **medium**, sentez **xhigh**. `max` yalnız kullanıcı isterse.
