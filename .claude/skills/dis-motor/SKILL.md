---
name: dis-motor
description: Ek motorlar. Codex CLI (OpenAI) tasarım, ikinci göz denetim ve münazara katılımcısı olarak; agy (Google Antigravity CLI) angarya ve ek kota olarak. Görevi kart dosyasıyla arka planda verir, yalnız rapor dosyasını okur, iddiayı git diff ile sınar. "Codex'e ver", "agy'ye ver", "tasarımı Codex yapsın", "angaryayı agy'ye ver", "kota dar, başka motor" dendiğinde ya da planlamada motor seçerken kullan.
---
# Dış motorlar

Hangi motorun kurulu olduğu `CLAUDE.md` → Proje bilgisi satırında yazar (`Codex: var|yok`, `agy: var|yok`). Yoksa önerme.
Giriş (login) kullanıcının işidir; komut "giriş gerekli" derse kullanıcıya `codex login` ya da agy girişini kendi terminalinde yapmasını söyle.

## Ne zaman hangisi
| İş | Motor | Neden |
|---|---|---|
| UI/UX yerleşimi, görsel stil, ekran tasarım belgesi, tema/token önerisi | **Codex** | tasarımda güçlü, farklı bakış |
| Ayrıştırıcı / kriptografi / güvenli yazma kodunun ikinci göz denetimi | **Codex** (salt okur) | güvenilmeyen girdide farklı kör nokta |
| İkinci göz denetim (önemli dalga) | **Codex** (salt okur) | "denetçi farklı motordur" ilkesini gerçekten uygular |
| Münazara personası | Codex ya da agy (salt okur) | farklı kör nokta |
| Toplu veri doldurma, uzun kaynak/video/web özeti, log analizi, Türkçe metin, test verisi | **agy** (`gemini-3.8-flash-medium`) | ucuz, Claude kotasını yemez |
| Claude kotası dar, kart net bir kodlama işi | **agy** (`claude-sonnet-5-5-medium`) | ek limit |

## Komutlar (proje kökünde, Bash `run_in_background: true` ile)
Codex, salt okuma (tasarım belgesi, denetim, görüş):
```
codex exec --skip-git-repo-check -s read-only -C "<PROJE_KOKU>" -o .fabrika/raporlar/<ID>-codex.md - < .fabrika/gorevler/<ID>.md
```
Codex, yazarak (yalnız kartın Sahip dosyaları): aynı komut, `-s read-only` yerine `-s workspace-write`.
Codex ile hazır inceleme: `codex exec review` (çalışma ağacındaki değişiklikleri inceler; çıktıyı `-o` ile dosyaya al).

agy, yazarak:
```
agy -p "Kartın: .fabrika/gorevler/<ID>.md. Oku ve uygula. Yalnız kartın Sahip listesindeki dosyalara yaz. Raporu .fabrika/raporlar/<ID>-agy.md dosyasına en çok 10 satır yaz." --model gemini-3.8-flash-medium --mode accept-edits --output-format text > .fabrika/raporlar/<ID>-agy.log 2>&1
```
agy, salt okuma: `--mode accept-edits` yerine `--mode plan`. Modelleri görmek: `agy models`.

## Kurallar
- **Asla:** Codex'e `--dangerously-bypass-approvals-and-sandbox` ya da `-s danger-full-access`; agy'ye `--dangerously-skip-permissions`.
  (Koruma kancası bunları zaten engeller.)
- Dış motor da kartla ve **sahiplik** kuralıyla çalışır: aynı dalgada aynı dosyada iki motor olmaz. Kilitli kaynaklar ona da yasak.
- TASKS'ta `Motor: codex` ya da `Motor: agy·<model>` yaz. HANDOFF'ta hangi motorun ne yaptığını yaz.
- **Koruma kancası dış motorun içini görmez.** Codex `workspace-write` ve agy `accept-edits` kipinde proje içindeki her dosyaya
  yazabilir; sahiplik ve kilit burada yalnız talimattır. Bu yüzden: mümkünse salt okuma kipinde çalıştır (tasarım belgesi, öneri, yama);
  yazma kipinde dalgada o motorla aynı anda çalışan başka görev olmasın.
- **Rapor iddiadır:** iş bitince yalnız rapor dosyasını oku (log'u değil), sonra `git status --porcelain --untracked-files=all` ile
  değişen dosyaların Sahip listesiyle aynı olduğunu kontrol et. Sahip dışı ya da kilitli dosya değiştiyse görev **başarısız**:
  dosyaları geri alma (kanca `git restore`'u engeller), kullanıcıya göster ve kararı sor. Sonra normal denetim + doğrulama.
- Dış motor alt ajan değildir: sonucu CEO'ya kendiliğinden gelmez; arka plan görevi bitince bildirimi bekle, rapor dosyasını oku.
