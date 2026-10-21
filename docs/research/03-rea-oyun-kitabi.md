# 03 — rea (Reverse Engineer Anything) Oyun Kitabı

> **TL;DR:** rea'yı uygulamaya gömmüyoruz; **geliştirme sırasında** ajanın (Claude Code) MCP aracı olarak kullanıyoruz. Üç iş: (1) oyun dosyalarından şifre/parola adayı çıkarmak, (2) bilinmeyen formatların kayıt rutinini bulmak, (3) rakip sitelerin *davranışını* temiz-oda kurallarıyla belgelemek.
>
> **Sonraki adım:** kurulum onayı → `npx rea-agents setup` (≈5 dk).

---

## 1. rea nedir

- MCP sunucusu + CLI. Analiz yerelde çalışır; her bulgu kanıt (Evidence) ve sınırlamalarla döner. MIT lisanslı.
- Gereksinim: Node.js 22.x ≥ 22.19 (sende 22.23.1 ✓).
- Bize lazım olan yetenekler (✓ = Ghidra/IDA/Hopper gerektirmez):

| Hedef | rea aracı | Bizdeki kullanım |
|---|---|---|
| .NET derlemeleri ✓ | managed-code analysis — `#US` user strings, `ldstr` çapaları | Unity Mono: `Assembly-CSharp.dll` içindeki ES3 parolası |
| IL2CPP metadata ✓ | `global-metadata.dat` → `string_literals` (sürüm 24–39) | Unity IL2CPP parola adayları |
| Godot PCK ✓ | `inspect-artifact` / `extract-artifact` (format 2–4, şifresiz) | Oyun içeriği; anahtar kurtarmaz → KeyDot |
| Masaüstü kurulumları ✓ | runtime adayları, bileşen işaretleri | Motor tespit kurallarımız için veri |
| Web siteleri ✓ | `inspect_web_page` (`include_script_sources`), `export_web_scripts` → `analyze_javascript_application` | Rakip site davranış haritası |
| WASM | `inspect_wasm_artifact` (WABT 1.0.42 gerekir) | Sitelerin WASM modülleri |
| Native ikili | Ghidra / IDA / Hopper | Özel XOR/AES rutini olan oyunlar |

---

## 2. Senaryolar

### Senaryo A — ES3 parolası (en değerli)

1. Oyun klasöründe `<Oyun>_Data/Managed/Assembly-CSharp.dll` dosyasını bul. ⏱ 1 dk
2. Ajana: *"Bu DLL'deki user string'leri ve ES3Settings / ES3.Save çağrılarına yakın `ldstr` değerlerini listele."* ⏱ 3 dk
3. Adayları doğrula: `saveprism keys try <save.es3> --candidates adaylar.txt` (PBKDF2 + AES + düz metin sezgisi; M2-4'te gelir). ⏱ 1 dk
4. Doğrulanan anahtarı kanıtıyla (dosya yolu + yöntem + tarih) yerel anahtar kasasına yaz.

- IL2CPP oyunlarda 2. adım `global-metadata.dat` string literal'leri olur.
- Bu akış M7'de uygulama içi **Anahtar Bulucu** olur; rea burada prototip ve doğrulama aracıdır.

### Senaryo B — Bilinmeyen format

1. `saveprism inspect save.dat` → entropi, magic, katman tahmini.
2. Tahmin yoksa: rea ile oyun ikilisinde kayıt rutinini izle (dosya adı string'i → onu çağıranlar → şifre/sıkıştırma çağrıları).
3. Bulguyu `docs/formats/<oyun>.md` spesifikasyonuna yaz; mümkünse `.ksy` / `.hexpat` ekle.
4. Fixture + altın test → codec veya **tarif (recipe)** olarak oyun profiline ekle.

### Senaryo C — Rakip site davranış haritası (temiz oda)

Amaç kod almak değil; **hangi formatları, hangi uzantı/magic sinyalleriyle, hangi oyun→motor eşleşmeleriyle** desteklediklerini bilmek.

1. Kullanım şartlarını (ToS) oku. Tersine mühendisliği yasaklıyorsa yalnız herkese açık sayfa metniyle sınırlı kal.
2. `inspect_web_page` + `include_script_sources` ile tek yakalama yap; `export_web_scripts` → `analyze_javascript_application`.
3. Çıktıdan yalnız **olgu** çıkar: desteklenen uzantılar, tespit sırası, hangi oyunların anahtar gerektirdiği.
4. Olguları `docs/research/rakip-davranis.md`'ye düz yazıyla yaz (kod parçası yok).
5. Uygulamayı yazan oturum yalnız bu dokümanı + kamuya açık format belgelerini görür.

Kurallar:
- Kod, regex, tablo ve **anahtar listesi kopyalanmaz**. Gömülü oyun anahtarlarını oradan almıyoruz; hangi oyunun anahtara ihtiyaç duyduğunu öğrenip anahtarı **kendi oyun kopyamızdan** Senaryo A ile türetiyoruz. Kanıt zinciri temiz kalır.
- Hız sınırına saygı: tek yakalama, otomatik tarama yok.
- Sunucu tarafı işleme (saveeditonline.com yüklenen dosyayı sunucuda işliyor) zaten görünmez → yerine açık kaynak kütüphaneler ([02](02-format-ve-kutuphane-atlasi.md)).
- Birlikte çalışabilirlik amaçlı analiz birçok hukuk düzeninde korunur (ör. AB 2009/24/EC m.6, ABD DMCA §1201(f)). Bu bir hukuki görüş değildir.

---

## 3. Kurulum (onay sonrası)

```bash
npx rea-agents setup
```

1. Claude Code'u seç, önerilen değişiklikleri incele, onayla.
2. Ajanı yeniden başlat.
3. Native analiz şimdilik gerekmez. Gerekirse Ghidra (JDK ister); rea Windows Ghidra desteğini "deneysel" olarak etiketliyor.

> ⚠️ Yalnız resmi kaynaklar: `morluto/rea`, rea.tools, npm `rea-agents`. skills.sh'te benzer adlı kopyalar var (ör. `zouineoualid/rea-reverse-engineering`, `gryszzz/reverse-engineer-everything`).

---

## 4. Sorumlu kullanım

Yalnız sahip olunan oyunlar ve tek oyunculu kayıtlar. Çevrimiçi/çok oyunculu oyunlar, hile önleme (anti-cheat) ve DRM kapsam dışı. rea'nın kendi DISCLAIMER'ı ile uyumlu.

---

## Kaynaklar

- [morluto/rea](https://github.com/morluto/rea) — README, `docs/managed-code-analysis.md`, `docs/il2cpp-metadata.md`, `docs/godot-pck.md`, `docs/website-script-export.md`, `docs/wasm-artifacts.md`
- [rea.tools](https://rea.tools/)
