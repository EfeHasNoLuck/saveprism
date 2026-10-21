# Fabrika — Günlük Akış (SaveLens)

> **TL;DR:** Ana oturum CEO'dur: planlar, alt ajanlara dağıtır, kanıt toplar. Sen iki kapıda karar verirsin: **KAPI-1** (planı onayla) ve **KAPI-2** (uygulamada/oyunda dene).
>
> **Sonraki adım:** M0-1 kararları ve M0-4 (Rust) bitince `fabrikayı başlat: M0 Grup B` yaz.

Kaynak: "Claude Code Çalışma Kiti v2 (Ders 3, öğrenci sürümü)" — Unity'ye özgü parçalar çıkarılıp SaveLens'e uyarlandı (2026-10-21).

---

## 1. Günlük akış

1. `fabrikayı başlat: <ROADMAP görevi ya da grubu>` yaz (ör. `M1 Grup A`). CEO talebi netleştirir, **dalga planı** çıkarır.
2. **KAPI-1:** planı oku, `dalga 1'i onaylıyorum` de (ya da değiştirttir). Onaysız ajan başlamaz.
3. Dalga paralel yürür: kodcular yazar, denetçi okur, CEO `kontrol.sh` kanıtını koşar.
4. **KAPI-2:** uygulamada/oyunda dene, gördüğünü yaz ("file1.rpgsave açıldı, altını 9999 yaptım, oyunda göründü"). CEO işaretler, commit'ler.
5. Status line **%55** gösterince `devret` de. Bilgisayardan ayrılacaksan `/afk hazırla`. Pahalı karar varsa `münazara kur: <karar>`.

## 2. Kopyala-yapıştır istemler

| Ne zaman | Yaz |
|---|---|
| Yeni iş | `fabrikayı başlat: M1 Grup A` |
| Planı onayla | `dalga 1'i onaylıyorum` |
| Doğrulama | `Denedim: <ne gördün, hata varsa ham metin>. Dalgayı kabul ediyorum.` |
| Devir | `devret` |
| Yeni pencerede | `handoff/ içindeki en yeni HANDOFF dosyasını oku, "Tam sonraki adım" 1'den devam et.` |

## 3. Kim ne yapar

| Rol | Model | İş |
|---|---|---|
| CEO (ana oturum) | Opus 5.5 · xhigh | Planlar, dağıtır, git'e yazar, kanıt koşar. Kod yazmaz |
| kodcu | haiku (zor işte sonnet) | Tek kart, yalnız Sahip dosyaları |
| denetci | sonnet | Salt okur; KRITIK/YUKSEK/ORTA/DUSUK bulgu |
| dalga-sefi | sonnet | 3+ görevlik dalgayı yürütür |
| tarayici | haiku | Toplu, mekanik işler |
| Codex / agy | dış motor | Tasarım + ikinci göz / angarya + ek kota |

## 4. Makineyle zorlanan kurallar (`.claude/hooks/koruma.sh`)

1. Geri alınamaz git (`reset --hard`, `clean -f`, `push --force`, `stash drop/clear`, `branch -D`) engellenir.
2. Özyinelemeli/toplu silme (`rm -r`, `Remove-Item -Recurse`, `rd /s`) engellenir.
3. Kilitli: `fixtures/samples/**`, `LICENSE`, `.claude/settings*.json`, `.claude/hooks/`, `.claude/araclar/`.
4. Proje dışına yazma engellenir (istisna: Claude plan/bellek dosyaları ve geçici klasör).
5. Dış motorlara tehlike bayrağı (`--dangerously-*`, `danger-full-access`) engellenir.

Korkuluktur, kum havuzu değildir. Asıl geri dönüş noktası git commit'leridir.

## 5. Kanıt komutu

```bash
bash .claude/araclar/kontrol.sh
```

- `rust`: cargo fmt --check · clippy -D warnings · test · `web`: pnpm typecheck · lint · test · varsayılan: ikisi.
- Çıkış: 0 geçti · 1 hata · 3 çalıştırılacak bir şey yok (ör. Rust henüz kurulu değil).

## 6. Kiti güncellemek (kilitli dosyalar)

Kilitli dosyaları Claude değiştiremez. Güncelleme gerektiğinde Claude'u kancasız aç:

```powershell
$env:KORUMA="kapali"; claude; Remove-Item Env:KORUMA
```

Kaldırma: kurulum commit'ini `git revert` et.
