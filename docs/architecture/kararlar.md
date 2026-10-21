# Karar Kayıtları (ADR-lite)

> **TL;DR:** Her karar 5 satır: durum, karar, neden, bedel, alternatif. "Açık" olanlar senin kararını bekliyor; "Önerildi" olanlar itiraz yoksa M0-1'de "Kabul" olur.
>
> **Sonraki adım:** Açık kararlar (ADR-009, ADR-012) — [NOW.md](../../NOW.md).

Durumlar: **Kabul** · **Önerildi** · **Açık** · **Değişti**

---

### ADR-001 — Masaüstü kabuk: Tauri 2 · Önerildi
- **Karar:** Tauri 2 + React/TS.
- **Neden:** küçük kurulum, Rust çekirdeğiyle doğrudan bağ, resmi updater, yetki modeli.
- **Bedel:** WebView farkları (özellikle Linux WebKitGTK).
- **Alternatif:** Electron (ağır), Python + pywebview (paketleme/antivirüs sorunları).

### ADR-002 — Çekirdek dili: Rust · Kabul (2026-10-21)
- **Karar:** tüm codec'ler Rust crate'lerinde.
- **Neden:** güvenilmeyen ikili veri, büyük dosya performansı, `uesave` yerel, CLI/MCP/WASM için tek kod tabanı.
- **Bedel:** Rust araç zinciri kurulumu (sende yok); RPGSaveEditor'ün TS pickle/Marshal yazıcılarını port etmek.
- **Alternatif:** TS codec'ler (RPGSaveEditor kodunu doğrudan kullanarak daha hızlı başlangıç, ama büyük dosyada yavaş ve CLI/MCP için Node gerekir).

### ADR-003 — Belge Rust'ta, UI tembel görünüm · Önerildi
- **Karar:** IPC'den yalnız görünen düğümler geçer; arama Rust'ta.
- **Neden:** yüz MB'lık Palworld/GVAS kayıtları web araçlarında donuyor; bizim farkımız.
- **Bedel:** daha fazla IPC çağrısı tasarımı.
- **Alternatif:** tüm JSON'u UI'ya göndermek (basit ama ölçeklenmez).

### ADR-004 — Yazma: yedek + atomik değiştirme + round-trip koruması · Önerildi (değişmez kural)
- **Karar:** her yazmadan önce yedek; geçici dosya + atomik değiştirme; açılışta ve yazım sonrası round-trip doğrulaması.
- **Neden:** kullanıcı güveni; kayıp save geri gelmez.
- **Bedel:** biraz daha yavaş kaydetme.
- **Alternatif:** yok.

### ADR-005 — Codec = kod, oyun = veri (profil) · Önerildi
- **Karar:** motor formatları Rust'ta; oyuna özel yollar/tarifler/etiketler TOML profillerinde.
- **Neden:** topluluk Rust bilmeden katkı verir; codec sayısı az, oyun sayısı binlerce.
- **Bedel:** profil şeması sürümlemesi gerekir.
- **Alternatif:** her oyun için kod modülü (SaveSmith yaklaşımı; ölçeklenmez).

### ADR-006 — Lisans hijyeni · Kabul
- **Karar:** proje MIT. GPL/AGPL/LGPL/SSPL bağımlılık yasak (cargo-deny). Apache-2.0 / MPL-2.0 NOTICE ile serbest.
- **Neden:** MIT topluluğa en geniş kullanım hakkını verir; copyleft bağımlılık bunu bozar.
- **Bedel:** bazı hazır kütüphaneler (ör. GPL `nrbf-parser`) yerine kendi kodumuz.
- **Alternatif:** —

### ADR-007 — Temiz oda tersine mühendislik · Önerildi
- **Karar:** kapalı kaynak site/paketlerden kod, regex, tablo veya anahtar listesi kopyalanmaz; yalnız davranış olguları belgelenir. Anahtarlar sahip olunan oyun dosyalarından türetilir.
- **Neden:** telif ve kullanım şartı riskini sıfıra yaklaştırmak; kanıt zinciri.
- **Bedel:** biraz daha yavaş format kapsamı.
- **Alternatif:** —

### ADR-008 — Dil · Kabul (2026-10-21)
- **Karar:** sohbet TR; repodaki her şey EN (kod, yorum, commit, dokümanlar). Türkçe kullanıcılar için `README.tr.md` ve uygulamada TR çeviri.
- **Neden:** topluluk katkısı EN ile en geniş kitleye açılır.
- **Bedel:** mevcut TR planlama dokümanlarının çevirisi (M0-12; ucuz motor agy'ye uygun iş). Yeni dokümanlar şimdiden EN.

### ADR-009 — Profillerde anahtar dağıtımı · Açık
- **Öneri:** başlangıçta profiller yalnız **bulma yöntemini** taşır; anahtar kullanıcının yerel kasasında. Hukuki görüş sonrası yeniden değerlendir.
- **Alternatif:** bilinen anahtarları kanıt kaynağıyla profillere eklemek (kullanıcı için daha kolay, risk daha yüksek).

### ADR-010 — RPGSaveEditor ilişkisi · Kabul (2026-10-21)
- **Karar:** kimseye muhtaç olmadan ilerliyoruz, ama iş birliğine açığız. **v0.1 yayınlandıktan sonra** eşit seviyede bir not gönderilir ve somut bir **ortak çözüm havuzu** önerilir: paylaşılan test kayıtları (fixture) + format notları ([taslak](../research/outreach-rpgsaveeditor.md)). Apache-2.0 kodu gerekirse atıfla uyarlanır (izin gerekmez).
- **Neden:** kapsamlar farklı (o: RPG Maker ailesi, biz: tüm motorlar), bu yüzden rekabet yok; ortak havuz iki projenin de test ve bakım yükünü azaltır. Çalışan bir ürünle gelen teklif ciddiye alınır.
- **Alternatif:** şimdi yazmak (gösterecek bir şey yok), hiç yazmamak (iki tarafa da yarayacak bir kapıyı kapatır).

### ADR-011 — Proje adı · Kabul (2026-10-21)
- **Karar:** **SavePrism**. Kullanıcı önerdi, iki aday aynı ölçütlerle karşılaştırıldı, kullanıcı "hangisi daha büyük katkı sağlıyorsa onu" dedi. Slug'lar: repo `saveprism`, crate'ler `saveprism-*`, CLI `saveprism`, hata kodları `SP-*`.
- **Neden:** GitHub'da hiç eşleşme yok, org adı ve .com/.app/.dev/.gg/.io boş, Google'da marka araması boş (aramayı ilk günden sahipleniyoruz), güçlü görsel kimlik (prizma logosu). Ayrıntı: [05-naming.md](../research/05-naming.md) §6.
- **Bedel:** "PRISM" yazılım markalarında kalabalık bir kelime → public öncesi marka araması (TÜRKPATENT/EUIPO/USPTO) şart. "save prism" araması Prisma'ya kayabiliyor.
- **Elenenler:** SaveStack (.com ve org dolu, görsel kimlik zayıf), SaveLore, Save-R ("Save-Rite" çakışması), OmniSave, Savecraft (savecraft.gg bir AI kayıt asistanı), SaveForge, Quicksave, Resave, Savepad, Savix, Saver, Savior.
- **Uygulandı:** M0-11 (yeniden adlandırma, 2026-10-21).

### ADR-012 — macOS imzası · Açık
- **Öneri:** v0.1'de imzasız macOS sürümü + talimat; talep olursa Apple Developer ($99/yıl).
- **Alternatif:** macOS'u ilk sürümde hiç yayınlamamak.

### ADR-013 — Hata modeli ve test stratejisi · Önerildi
- **Karar:** crate başına tipli hatalar, kararlı `SP-<ALAN>-<NNN>` kodları, kütüphanelerde panik yok (lint ile), altın round-trip korpusu + snapshot + property + fuzz testleri, "önce başarısız test, sonra düzeltme" kuralı. Ayrıntı: [QUALITY.md](QUALITY.md).
- **Neden:** hatayı kullanıcıya ulaşmadan yakalamak; ulaşırsa dakikalar içinde doğru dosyaya inmek.
- **Bedel:** M1'de birkaç saatlik altyapı; sonraki her codec bunu hazır bulur.

### ADR-014 — Kod haritası · Önerildi
- **Karar:** [MAP.md](../MAP.md) belirtiden dosyaya en kısa yol; her yeni modül/hata öneki aynı değişiklikte eklenir (denetçi kontrol eder). Kod büyüyünce `/graphify` ek olarak kullanılır.
- **Neden:** arama yerine doğrudan gitmek → daha az token, daha hızlı düzeltme.

### ADR-015 — Risk kaydı ve kapsam dışı listesi · Önerildi
- **Karar:** [RISKS.md](../RISKS.md) her riske bir strateji (önle / tespit et / sınırla / kabul et) atar; §3 Non-Goals listesi yalnız ADR ile değişir.
- **Neden:** "ben bunu istememiştim" sürprizlerini baştan kapatmak.
