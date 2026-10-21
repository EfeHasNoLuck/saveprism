# 01 — Pazar ve Açık Kaynak Taraması ("Boşa kürek çekme" kontrolü)

> **TL;DR:** Tüm motorları tek, modern, yerel ve ücretsiz bir masaüstü arayüzde birleştiren aktif bir açık kaynak proje **yok**. En yakın iki proje birbirinin eksiğini kapatıyor: biri masaüstü ama dar kapsamlı (RPGSaveEditor), diğeri geniş kapsamlı ama web + reklamlı + lisans dosyası yok (paradoxie/saveeditor). **Karar: inşa et**, ama RPGSaveEditor ile iş birliği kapısını önce aç.
>
> **Sonraki adım:** [NOW.md](../../NOW.md) karar #3 (iş birliği mesajı).

Tarih: 2026-10-21 · Yöntem: GitHub araması (`gh`, yıldız sıralı, EN/ZH/JA terimler), web araması, rakip sitelerin herkese açık sayfaları, adayların kaynak ağaçları ve sürüm geçmişi.

---

## 1. Ticari / kapalı web servisleri

| Servis | Kapsam | Kısıt / Model | Gizlilik |
|---|---|---|---|
| saveeditonline.com | RPG Maker 2000→MV, Ren'Py, Unity, UE4–UE5.7 GVAS, Godot, HTML oyunları, Wolf RPG, Flash `.sol`, Java serialized | Dosya boyutu limiti var (sayı sayfada yok), Patreon destekli | Dosya **sunucuya yükleniyor**, site beyanına göre 1 gün sonra siliniyor |
| saveeditor.online | Ren'Py, Unity `.dat`, RPG Maker MV/MZ/XP/VX/VXA, Wolf, Naninovel, TyranoScript, UE GVAS, Twine, QSP, Pokémon Gen3, JSON | Ücretsiz 10 MB / ücretli 20 MB; ücretsizde yükleme bekleme süresi; aylık $2.50 / $5.50 / $27.50; **Ren'Py imzalama ücretli planda** | Editör tarayıcıda; Ren'Py imzalama için kullanıcının `security_keys.txt` dosyası **siteye yükleniyor** |

**Çıkarım:** Bu sitelerin değeri format kapsamında. Arayüz, limitler ve gizlilik zayıf. Ren'Py imzalamayı yerelde, kullanıcının kendi anahtarıyla ücretsiz yapmak doğrudan bir farklılaşma.

---

## 2. Açık kaynak "evrensel" adaylar

### En yakın iki aday

| | **truongthang2211/RPGSaveEditor** | **paradoxie/saveeditor** (saveeditor.top) |
|---|---|---|
| Tür | Masaüstü — Tauri 2 + React 18 + TypeScript | Web — Astro + React, istemci tarafı |
| Kapsam | RPG Maker 2000→MZ, Ren'Py 7/8, Twine/SugarCube, Wolf RPG, JSON, ES3 (yalnız şifresiz), Naninovel | RPG Maker MV/MZ, Unity (PlayerPrefs/JSON/XML), Ren'Py (deneysel), UE GVAS, GameMaker, Naninovel, NRBF + JSON/XML/ZIP/SQLite/MsgPack/CBOR/BSON/YAML/TOML |
| Durum | Aktif — v1.6.0 (2026-10-08), 69★, imzalı otomatik güncelleme, tek geliştirici | Aktif — son push 2026-08-31, 23★ |
| Lisans | Apache-2.0 | README "MIT" diyor ama **repoda LICENSE dosyası yok** → hukuken belirsiz, kod alınamaz |
| Güçlü yan | Kayıpsız Marshal/pickle yazıcı, güçlü arama (`>1000`, `party.**.@gold`), kayıt karşılaştırma ("değişen/artan"), açık/koyu tema | Format genişliği, fixture tabanlı testler |
| Eksik | Unreal, şifreli ES3, Unity PlayerPrefs/BinaryFormatter, Godot, GameMaker, hex görünüm, AI dışa/içe aktarım, macOS/Linux, i18n | Masaüstü değil, reklam/AdSense + SEO odaklı, lisans belirsiz |

### Diğerleri (kapsam dışı veya durgun)

| Proje | Not |
|---|---|
| gundamff/SaveSmith (Tauri 2 + Vue 3, MIT, 6★) | **Oyuna özel** modüller, motor-genel değil. Güvenli yazma deseni örnek alınacak: yedek → geçici dosya → atomik değiştirme, son 10 yedek |
| bikai9289/saveeditor | paradoxie fork'u (savefiletool.com), ek değer yok |
| Stormwave/SaveEditor | 2023'ten beri durgun, GPL-3.0 |
| Xeeynamo/KingdomSaveEditor | Oyuna özel (KH/FF7R/Persona), 2022'den beri durgun, GPL-3.0 |
| Poiki/rpgsave-editor | tkinter, tek sürüm, lisans yok |

> ⚠️ **Şüpheli repolar:** `pebblebakerspell/save-editor-universal-tool` (0★, bugün güncellenmiş, `main.py` + `Program.cs` + `index.html`) ve `Sepiolina/Offline-Save-Editor` (0★, yalnız README). "Evrensel araç" adıyla açılan sıfır yıldızlı repolar bilinen bir zararlı yazılım yemi kalıbı. **İndirilmedi, çalıştırılmamalı.**

---

## 3. Motor bazlı en iyi parçalar (birleştirilecek havuz)

Ayrıntı, teknik doğrulamalar ve lisanslar: [02-format-ve-kutuphane-atlasi.md](02-format-ve-kutuphane-atlasi.md).

1. **Unreal:** `trumank/uesave` (Rust, MIT, 485★), `cheahjs/palworld-save-tools` (MIT, 866★)
2. **Unity ES3:** `cppbear/Es3Offline`, `Chaobs/unity-es3-cracker` (parola bulma, MIT)
3. **Godot:** `GDRETools/gdsdecomp` (MIT, 4.3K★), `Titoot/KeyDot` (anahtar çıkarma, MIT)
4. **RPG Maker XP–VXA:** `alox-48` (Ruby Marshal, Rust, MPL-2.0)
5. **RPG Maker / Ren'Py yazıcıları:** RPGSaveEditor `src/formats/*` (Apache-2.0)

---

## 4. Kritik karar

Soru: *"Tüm ihtiyaçları karşılayan, temiz arayüzlü, açık kaynak bir proje var mı?"* → **Hayır.**

| Seçenek | Artı | Eksi |
|---|---|---|
| A. Yalnız RPGSaveEditor'e katkı | Tekrar yok | Adı ve vizyonu "RPG"; Unreal/Unity/Godot/hex kapsam dışı kalabilir; tek geliştirici darboğazı; yalnız Windows |
| B. Sıfırdan, bağımsız | Tam kontrol | RPG Maker/Ren'Py'da kanıtlanmış işi yeniden yazmak |
| **C. Hibrit (öneri)** | MVP boşluklara odaklanır (Unreal, şifreli ES3, Unity, Godot, katmanlı çözücü, AI köprüsü); RPG Maker/Ren'Py için Apache-2.0 kodu NOTICE ile uyarlanır veya ortak codec kütüphanesi kurulur | Lisans başlıklarına özen; iletişim zamanı |

**Önerilen ilk hamle:** RPGSaveEditor'de nazik bir Discussion/Issue (taslak: [outreach-rpgsaveeditor.md](outreach-rpgsaveeditor.md)). Cevap beklenmeden M1–M2 boşluk formatlarıyla başlanır.

---

## 5. Konumlandırma

> **Motor-bağımsız, katman katman çözen, kayıpsız geri paketleyen, AI'a hazır JSON köprüsü olan, ücretsiz ve tamamen yerel masaüstü save editörü.**

5 farklılaştırıcı:

1. **Round-trip garantisi** — dosya açılınca "değiştirmeden geri yaz = aynı bayt mı?" testi; değilse salt-okunur.
2. **Katmanlı tarif (Recipe)** — Base64 → AES → GZip → JSON gibi zincirleri otomatik tahmin veya elle kurma (CyberChef mantığı).
3. **AI köprüsü** — "AI için kopyala" (JSON), "AI'dan yapıştır" (JSON / JSON Patch + fark önizleme), CLI, ileride MCP.
4. **Anahtar Bulucu** — oyunun kendi dosyalarından (DLL / IL2CPP metadata / GDScript) parola adayı çıkarıp doğrulama.
5. **Oyun profilleri** — topluluk Rust bilmeden, PR ile oyun ekler (yol, tarif, hızlı alanlar).

---

## Kaynaklar

- [truongthang2211/RPGSaveEditor](https://github.com/truongthang2211/RPGSaveEditor)
- [paradoxie/saveeditor](https://github.com/paradoxie/saveeditor)
- [gundamff/SaveSmith](https://github.com/gundamff/SaveSmith)
- [saveeditonline.com](https://www.saveeditonline.com/) · [saveeditor.online](https://saveeditor.online/)
- [trumank/uesave](https://github.com/trumank/uesave) · [cheahjs/palworld-save-tools](https://github.com/cheahjs/palworld-save-tools)
- [cppbear/Es3Offline](https://github.com/cppbear/Es3Offline) · [Titoot/KeyDot](https://github.com/Titoot/KeyDot) · [GDRETools/gdsdecomp](https://github.com/GDRETools/gdsdecomp)
