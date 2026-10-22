---
name: devret
description: Devir (HANDOFF). Bağlam %55'e gelince, kullanıcı "devret" deyince, gün/özellik bitince ya da 5 saatlik kota %85'te çalışır. Her devirde handoff/ altında YENİ dosya yazar (eskisinin üstüne yazmaz), dört başlık, kanıtsız satırı ayıklar, TASKS ve NOW.md'yi günceller, commit atar ve kullanıcıya kalan adımları söyler.
argument-hint: "[neden: bağlam | gün sonu | özellik bitti | kota]"
---
# Devret

Devir kartı: 0 kullanıcı "devret" der · **1–5 sen** · 6–8 kullanıcı.

1. **Yeni iş açma.** Yeni dalga, yeni alt ajan yok.
2. Elindeki iş 1–2 küçük adımlıksa bitir; değilse olduğu yerde bırak ve Denenmedi'ye yaz.
3. Koşan alt ajan ve dış motor varsa bitmesini bekle, raporunu al.
4. **HANDOFF yaz:** `handoff/HANDOFF-YYYYMMDD-N.md` (aynı gün için sıradaki N; `handoff/` içine bak). Şablon: `.fabrika/HANDOFF-SABLON.md`.
   Önceki HANDOFF'u oku, güncel olmayanı taşıma. **Eski dosyanın üstüne yazma.**
   - Genel durum: `bitti` yalnız Çalışmayan boşsa ve her şey kanıtlıysa; değilse `yarım`. Neden devrediyorum: bağlam %X / gün sonu / ...
   - **Çalışan (kanıtla):** her satırda kanıt = dosya:satır, koşulan komut + çıktı özeti (ör. `kontrol.sh rust → 3/3 adım geçti`)
     ya da kullanıcının yazdığı uygulama/oyun gözlemi + TASKS `[x]`. Kanıt yoksa buraya yazma.
   - **Çalışmayan (neden):** hata, ham çıktı, tahmin. **Belge tutarsızlığı da buraya** (ör. MIMARI ile kod farklı sözleşme).
   - **Denenmedi:** neden denenmedi (sıra gelmedi / uygulamada sınanmadı / karar bekliyor / kota). `[?]` kararları ve AFK'da park edilenler.
   - **Tam sonraki adım:** numaralı, her adımda sahip `[ben | CEO | kodcu | denetci | tarayici | codex | agy]`, dosya ve komut düzeyinde.
     "Devam et" adım değildir.
   - Hangi motor ne yaptı (Codex/agy kullanıldıysa).
   - En sona: `Yeni pencereye yapıştır: handoff/ içindeki en yeni HANDOFF dosyasını oku, "Tam sonraki adım" 1'den devam et.`
   - En çok 60 satır. Emin olmadığını `(emin değilim)` diye işaretle. Şifre, anahtar, kişisel bilgi yok (oyun parolaları dahil).
5. **Kanıtsız satırı ayıkla:** Çalışan'daki her satırı yeniden oku; kanıtı olmayanı Denenmedi'ye taşı. TASKS.md işaretlerini güncelle.
   **NOW.md'yi güncelle:** Faz, Son kazanım (yalnız kanıtlı iş), Sıradaki tek adım (HANDOFF'taki 1. adım), Park alanı.
   Sonra **yalnız yönetim dosyalarını** commit'le: `git add TASKS.md NOW.md docs handoff .fabrika` →
   `git commit -m "devir: HANDOFF-YYYYMMDD-N"`. Doğrulanmamış kod (`crates/`, `apps/`) commit'lenmez; çalışma ağacında kalır ve HANDOFF'ta
   "Denenmedi" altında dosya listesiyle yazılır (yeni pencere onu `git status` ile görür).

Kullanıcıya şunu söyle (kısa):
```
Devre hazırım: handoff/HANDOFF-YYYYMMDD-N.md
6) 60 saniye testi: dosyayı ilk kez görüyormuş gibi oku. İlk adımı kopyalayabiliyor musun? Çalışan'daki her satırda kanıt var mı?
7) /rename <kısa-ad>  sonra  /clear
8) Yeni pencereye: handoff/ içindeki en yeni HANDOFF dosyasını oku, "Tam sonraki adım" 1'den devam et.
```
"En yeni" = `handoff/HANDOFF-YYYYMMDD-N.md` kalıbında tarihi ve N'i en büyük olan.
Yeni pencerede ilk cevabın: iki cümlelik durum özeti + ilk adım. "Nerede kalmıştık?" diye sorman HANDOFF'un eksik olduğunu gösterir.

## Devamı kendin başlatırsan (yetki: CLAUDE.md "Bağlam ve kota")

Kullanıcı 6–8'i yapmaz. Sonunda ortada **tek açık oturum** kalır; eski oturum kalıntı olarak durmaz.
1. Commit ve push bitmiş olsun.
2. Yeni oturum: `start_session` (`context: "fresh"`, `use_worktree: false`), istem = 8. adımdaki cümle.
   Kota doluysa bunun yerine sıfırlanma saatine tek seferlik zamanlanmış görev (yan oturum değildir, 3'ü atla).
3. `detach_session <yeni oturumun id'si>`. Yapılmazsa 5. adım yeni oturumu da arşivler.
4. Kullanıcıya tek satır: `Devam burada: [başlık](#local_…). Bu oturumu kapatıyorum.`
5. `archive_session self`. Bu çağrı oturumu bitirir; ondan sonra bir şey yazma.
