# Karar Kayıtları (ADR-lite)

> **TL;DR:** Her karar 5 satır: durum, karar, neden, bedel, alternatif. "Açık" olanlar senin kararını bekliyor; "Önerildi" olanlar itiraz yoksa M0-1'de "Kabul" olur.
>
> **Sonraki adım:** Açık kararlar (ADR-008…012) — [NOW.md](../../NOW.md).

Durumlar: **Kabul** · **Önerildi** · **Açık** · **Değişti**

---

### ADR-001 — Masaüstü kabuk: Tauri 2 · Önerildi
- **Karar:** Tauri 2 + React/TS.
- **Neden:** küçük kurulum, Rust çekirdeğiyle doğrudan bağ, resmi updater, yetki modeli.
- **Bedel:** WebView farkları (özellikle Linux WebKitGTK).
- **Alternatif:** Electron (ağır), Python + pywebview (paketleme/antivirüs sorunları).

### ADR-002 — Çekirdek dili: Rust · Önerildi
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

### ADR-008 — Doküman dili · Açık
- **Öneri:** iç planlama TR (şimdiki dosyalar); public yüzey (README, CONTRIBUTING, uygulama varsayılanı) EN + TR çeviri.
- **Alternatif:** her şey EN (topluluk için en geniş), her şey TR (sana en rahat).

### ADR-009 — Profillerde anahtar dağıtımı · Açık
- **Öneri:** başlangıçta profiller yalnız **bulma yöntemini** taşır; anahtar kullanıcının yerel kasasında. Hukuki görüş sonrası yeniden değerlendir.
- **Alternatif:** bilinen anahtarları kanıt kaynağıyla profillere eklemek (kullanıcı için daha kolay, risk daha yüksek).

### ADR-010 — RPGSaveEditor ilişkisi · Açık
- **Öneri:** önce iş birliği mesajı ([taslak](../research/outreach-rpgsaveeditor.md)); cevap beklenmeden boşluk formatlarıyla başla.
- **Alternatif:** iletişim kurmadan Apache-2.0 kodu NOTICE ile uyarlamak; ya da hiç kullanmamak.

### ADR-011 — Proje adı · Açık
- Kod adı **SaveLens** (GitHub'da birebir ad yok, npm ve crates.io boş — 2026-10-21).
- Elenenler: SaveSmith (aynı alanda Çince bir proje var), SaveKit (npm dolu), SaveForge (aynı adlı şüpheli bir repo var).

### ADR-012 — macOS imzası · Açık
- **Öneri:** v0.1'de imzasız macOS sürümü + talimat; talep olursa Apple Developer ($99/yıl).
- **Alternatif:** macOS'u ilk sürümde hiç yayınlamamak.
