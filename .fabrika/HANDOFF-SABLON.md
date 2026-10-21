# HANDOFF: <iş / özellik adı> (<YYYY-AA-GG SS:DD>)

<!-- Dosya adı: handoff/HANDOFF-YYYYMMDD-N.md. Her devir YENİ dosya; eskisinin üstüne yazılmaz. En çok 60 satır. -->

Genel durum: **<yarım | bitti>**. Neden devrediyorum: <bağlam %X (/context) | gün sonu | özellik bitti | kota %X>.
Son commit: `<kısa hash> <mesaj>` · Açık dalga: <Dalga N, KAPI-1 onaylı/bekliyor>

## Çalışan (kanıtla)
<!-- Kanıt = dosya:satır, koşulan komut + çıktı özeti, ya da kullanıcının uygulama/oyun gözlemi + TASKS [x]. Kanıt yoksa buraya yazma. -->
| İş | Kanıt |
|---|---|
| <T1 ...> | <dosya yolu; `kontrol.sh rust` → 3/3 adım geçti; düzenlenen kayıt oyunda yüklendi (kullanıcı)> |

## Çalışmayan (neden)
<!-- Hata + ham konsol mesajı + tahmin. Belge tutarsızlığı da buraya. Yoksa "yok". -->
- **<ne>**: <ham hata / neden / tahmin>

## Denenmedi
<!-- Neden: sıra gelmedi / uygulamada sınanmadı / karar bekliyor [?] / kota / AFK'da park edildi. Yoksa "yok". -->
- <ne> (<neden>)

## Tam sonraki adım
<!-- Numaralı, sahipli [ben | CEO | kodcu | denetci | tarayici | codex | agy], dosya ve komut düzeyinde. "Devam et" adım değildir. -->
1. [<sahip>] <somut adım>
2. [<sahip>] <somut adım>

Motorlar: <Codex/agy kullanıldıysa hangi görevde ne yaptı; yoksa "yalnız Claude">

Yeni pencereye yapıştır: `handoff/ içindeki en yeni HANDOFF dosyasını oku, "Tam sonraki adım" 1'den devam et.`
