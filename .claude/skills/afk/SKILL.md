---
name: afk
description: AFK kipi. Kullanıcı başında değilken, KAPI-1'den geçmiş dalgaları onaysız kipte yürütür. "/afk hazırla" kullanıcı başındayken hazırlık ve kontrol yapar; "/afk başlat" AFK oturumunda yürütmeyi başlatır.
argument-hint: "hazırla | başlat"
---
# AFK kipi

AFK **yeni özellik planlamaz**. Yalnız `TASKS.md`'de `KAPI-1: onaylandı` olan dalgaları yürütür. KAPI-2 (uygulamada / oyunda doğrulama)
kullanıcı dönünce yapılır; AFK yalnız makine kanıtı (`kontrol.sh`) üretir.

## `/afk hazırla` (normal oturumda, kullanıcı başındayken)
1. `TASKS.md`'de onaylı ve bitmemiş dalga var mı? Yoksa dur: "Önce planla ve KAPI-1 ver."
2. `git status` temiz mi? Değilse göster, kullanıcının onayıyla `git add -A` → `git commit -m "AFK-öncesi <tarih>"`.
   Her durumda başlangıç noktasını (`git rev-parse --short HEAD`) not et; 5. adımda `afk.md`'ye yazılır.
3. `bash .claude/araclar/kontrol.sh` bir kez koş: çıkış 0 ya da 1 olmalı (araçlar kurulu). Çıkış 3 ise AFK'da makine kanıtı üretilemez;
   kullanıcıya söyle (ör. Rust kurulu değil).
4. Kullanıcıya **tek mesajda** sor (AskUserQuestion):
   - Kalıcı eskalasyon tercihi (görev başarısız olursa, ben yokken): (a) görevi `[?]` park et, bağımsız işlere devam et *(önerilen)*
     · (b) effort'u bir kademe yükselt (en çok xhigh) · (c) bir üst motora geç (en çok opus).
   - Durma koşulu: en çok kaç dalga (önerilen: 2) ya da onaylı dalgalar bitene kadar.
   - CEO modeli Fable ve kullanım kredisiyle çalışıyorsa: harcama tavanı ayarlı mı? (Ayarlı değilse önce claude.ai'den tavan koymasını öner.)
5. Cevapları `.fabrika/durum/afk.md` dosyasına yaz (tercih, durma koşulu, onaylı dalgalar, başlangıç commit'i).
6. Kullanıcıya ver:
   ```
   Bu oturumu kapat (/exit). Proje klasöründe yeni terminalde:
   claude --permission-mode auto "/afk başlat"
   (Daha az güvenli seçenek: claude --dangerously-skip-permissions "/afk başlat")
   İlk kullanımda bir uyarı ekranı çıkar: kabul et, ilk dalganın başladığını gör, SONRA ayrıl.
   ```

## `/afk başlat` (AFK oturumunda)
1. `.fabrika/durum/afk.md` yoksa ya da içindeki başlangıç noktası `git log`'da yoksa DUR: "Önce normal oturumda /afk hazırla."
2. Her onaylı dalga için, sırayla (dalga içi paralel):
   1. `fabrika` skill'inin 4–5. adımları (kartlar, plan commit'i, dağıtım, denetim, sahiplik kontrolü). Kullanıcıya soru sorma. Boru hattı yok: dalgalar sırayla.
   2. `bash .claude/araclar/kontrol.sh`. Çıktıyı `.fabrika/raporlar/D<N>-kanit.md`'ye yaz.
   3. Çıkış 0 → `git add -A` → `git commit -m "AFK D<N>: <ID'ler> (makine kanıtlı, kullanıcı doğrulaması bekliyor)"`. Görevler `[~]` kalır
      (KAPI-2 kullanıcının), Kanıt satırına komut çıktısı özeti.
   4. Hata → `fabrika` §7'deki deneme sayacı + `afk.md`'deki kalıcı tercih. 3. denemede `[!]`. Park edilen görev `[?]`.
   5. Ara HANDOFF: `devret` skill'iyle yeni dosya (limit işi keserse en çok bir dalga kaybolur). Kontrolden geçemeyen dalganın kodu
      commit'lenmez; çalışma ağacında kalır ve HANDOFF'ta listelenir. Sonraki dalga o dosyalara dokunuyorsa başlatma (park et).
3. Durma koşulu, onaylı dalgaların bitmesi, bağlam %55 ya da kota uyarısı → son HANDOFF + `.fabrika/raporlar/AFK-<tarih>.md`
   (yapılan dalgalar, commit'ler, başarısız/park edilen görevler, kullanıcıyı bekleyen kararlar). Sonra dur.

## AFK'da asla
`git push` · yeni repo/sürüm yayınlama · paket kurma (cargo add, pnpm add, npm, pip) · kilitli kaynak (`fixtures/samples/**`, `LICENSE`) ·
`.claude/` ayarlarını ya da kancaları değiştirme · onaysız görev ya da yeni dalga planlama · `max` effort · alt ajana Fable ·
dış motora tehlike bayrağı · dosya/klasör silme. Koruma kancası bunların çoğunu makineyle engeller; engellenirsen dolanmaya çalışma,
işi park et ve rapora yaz.
