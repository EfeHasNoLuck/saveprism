# SaveLens *(kod adı)*

Oyun kayıt dosyalarını — Unreal, Unity, RPG Maker, Ren'Py, Godot ve daha fazlası — tek uygulamada açıp temiz bir ağaç/JSON olarak düzenleyen, sonra **orijinal formatına birebir geri paketleyen**; ücretsiz, tamamen yerel ve MIT lisanslı masaüstü aracı.

**Durum:** araştırma ve planlama tamam → M0 kararları bekleniyor. Güncel durum ve sıradaki adım: **[NOW.md](NOW.md)**

---

## Neden?

1. **Web servisleri kısıtlı:** yükleme limitleri, bekleme süreleri, abonelik duvarları; bazıları dosyayı sunucuya gönderiyor.
2. **Açık kaynak tarafı parçalı:** her motor için ayrı CLI; en iyi masaüstü aracı yalnız RPG Maker/Ren'Py ailesine odaklı ve yalnız Windows.
3. **AI çağı için tasarlanmamış:** kayıtları bir AI'ın güvenle okuyup düzenleyebileceği temiz JSON'a çeviren, değişikliği fark önizlemesiyle geri uygulayan bir araç yok.

Ayrıntı: [docs/research/01-pazar-taramasi.md](docs/research/01-pazar-taramasi.md)

## Belgeler

| Belge | İçerik |
|---|---|
| [NOW.md](NOW.md) | Şu an neredeyiz, sıradaki tek adım |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestone'lar ve mikro görevler |
| [docs/architecture/MIMARI.md](docs/architecture/MIMARI.md) | Teknoloji yığını ve mimari |
| [docs/architecture/kararlar.md](docs/architecture/kararlar.md) | Karar kayıtları (ADR) |
| [docs/research/](docs/research/) | Pazar, formatlar, rea, skills.sh/i-have-adhd |

## Lisans

[MIT](LICENSE). Yalnız sahip olduğunuz oyunların tek oyunculu kayıtları için tasarlanmıştır; çevrimiçi oyunlar, hile önleme ve DRM kapsam dışıdır.
