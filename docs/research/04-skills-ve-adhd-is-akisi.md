# 04 — skills.sh Ekosistemi ve i-have-adhd İş Akışı

> **TL;DR:** Skill'ler ajanın prosedür bilgisidir ve aynı zamanda **prompt kodudur** (tedarik zinciri riski). Proje kapsamında, resmi kaynaklı, küçük bir çekirdek set kuruyoruz. i-have-adhd'yi üç katmanda kullanıyoruz: ajan çıktısı, doküman biçimi, yol haritası mekaniği.
>
> **Sonraki adım:** "Çekirdek set"i onayla → 5 komutla kurulum (≈5 dk).

---

## 1. skills.sh nedir

- Vercel'in ajan skill dizini ve liderlik tablosu. CLI: `npx skills add <owner/repo> [-s skill]`, `find`, `use`, `update`, `experimental_install` (lock dosyasından geri yükleme).
- `add` varsayılan olarak **proje kapsamına** kurar (`-g` global).
- Arama API'si Vercel OIDC token istiyor; tarama `npx skills find <sorgu>` ile yapıldı (2026-10-21).

**Güvenlik kuralları**

1. Kurmadan önce oku: `npx skills use <repo>@<skill>` veya GitHub'da `SKILL.md`.
2. Resmi/kurumsal kaynak tercih et (anthropics, vercel-labs, shadcn-ui, mattpocock, morluto, ayghri).
3. Proje kapsamında kur → tüm katkıcılar aynı seti görür.
4. Lock dosyasını commit'le.

---

## 2. Taranan alanlar ve seçimler

Kurulum sayıları skills.sh, 2026-10-21.

### Çekirdek set (öneri: M0-2'de kur)

| Skill | Kaynak | Kurulum | Neden |
|---|---|---|---|
| i-have-adhd | `ayghri/i-have-adhd` | 28K | Odak ve çıktı biçimi |
| reverse-engineer-anything | `morluto/rea` | 8.3K | rea iş akışı talimatları (`rea-agents setup` ile gelir) |
| shadcn | `shadcn-ui/ui` | 388K | Resmi; bileşen ekleme ve tema |
| frontend-design | `anthropics/skills` | 1M | Şablon görünmeyen arayüz |
| tdd | `mattpocock/skills` | 1.1M | Codec'ler için test-önce |

### İhtiyaç anında — geliştirme

| Alan | Skill | Kurulum | Not |
|---|---|---|---|
| Tauri | `nodnarbnitram/claude-code-extensions@tauri-v2` | 7.9K | Topluluk; kurmadan önce oku |
| Rust | `apollographql/skills@rust-best-practices` | 20.7K | |
| React | `vercel-labs/agent-skills@vercel-react-best-practices` | 824K | |
| UI kalitesi | `vercel-labs/agent-skills@web-design-guidelines` | 765K | |
| Test | `antfu/skills@vitest` | 42K | |

### İhtiyaç anında — RE, güvenlik, GitHub

| Alan | Skill | Kurulum | Not |
|---|---|---|---|
| Format RE | `wshobson/agents@protocol-reverse-engineering` / `@binary-analysis-patterns` | 12.6K / 10.8K | Senaryo B için |
| Güvenlik | `getsentry/skills@security-review` | 20.6K | Ayrıştırıcı güvenliği |
| Git ve sürüm | `addyosmani/agent-skills@git-workflow-and-versioning` | 55.7K | Conventional Commits |
| CI | `wshobson/agents@github-actions-templates` / `openai/skills@gh-fix-ci` | 17.3K / 11.3K | |
| Karar sorgulama | `mattpocock/skills@grill-me` | 1.4M | Mimari kararları zorlamak |

i18n için aday: `lobehub/lobehub@i18n` (4.6K) — içerik incelenmeden kurulmaz.

### Sende zaten var (tekrar kurma)

design-taste-frontend, emil-design-eng, high-end-visual-design, redesign-existing-projects, karpathy-guidelines, graphify, deep-research, code-review, security-review.

### Yıldızladığın repolardan bu projeye ilgili olanlar

1. `addyosmani/agent-skills`, `pbakaus/impeccable`, `Leonxlnx/taste-skill` — mühendislik ve tasarım skill'leri
2. `google/osv-scanner` — CI'da bağımlılık zafiyet taraması
3. `usestrix/strix` — güvenlik taraması (CI skill'i var)
4. `maximosovsky/readme-guidelines` — public README kontrol listesi
5. `KnifeLemon/Filee` — çevrimdışı, sürükle-bırak, çok formatlı dönüştürücü → UX referansı

### Kurulum komutları (onay sonrası, proje kapsamı)

```bash
npx skills add ayghri/i-have-adhd -a claude-code
npx skills add shadcn-ui/ui -s shadcn -a claude-code
npx skills add anthropics/skills -s frontend-design -a claude-code
npx skills add mattpocock/skills -s tdd -a claude-code
npx rea-agents setup
```

---

## 3. i-have-adhd → proje mekaniği

i-have-adhd, ajan çıktısını DEHB'li bir okurun eyleme geçebileceği biçime sokan 10 kurallık bir skill. Biz bu kuralları yalnız sohbette değil, **dokümanlarda ve planda** da uyguluyoruz.

| Kural | Projede karşılığı |
|---|---|
| 1. Önce sonraki eylem | Her doküman "TL;DR + Sonraki adım" ile başlar; NOW.md'nin ilk satırları |
| 2. Numaralı adımlar | Mikro görevler numaralı, her biri tek eylem |
| 3. Tek somut sonraki adım | Her görevde "▶ İlk hamle (≤2 dk)" |
| 4. Yan konuları bastır | NOW.md "Park alanı" + GitHub `later` etiketi |
| 5. Durumu her tur yeniden söyle | NOW.md + oturum başı protokolü ([AGENTS.md](../../AGENTS.md)) |
| 6. Somut süre | Her görevde ⏱ dakika |
| 7. Kazanımı görünür kıl | Her milestone'un "Demo" satırı + CHANGELOG |
| 8. Olgusal hata dili | Uygulamada hata = "Neden + Çözüm" |
| 9. Liste ≤5 | Görev grupları en fazla 5 madde |
| 10. Giriş/kapanış yok | Dokümanlarda dolgu cümlesi yok |

### Oturum protokolü (≈2 dk)

1. `NOW.md`'yi oku.
2. ROADMAP'ten **tek** mikro görev seç.
3. Bitince NOW.md'de "Son kazanım" ve "Sıradaki tek adım"ı güncelle, commit'le.

### Ürün UX'ine yansıması

1. Boş ekran = 3 numaralı adım: "1 Dosyayı sürükle · 2 Değeri bul · 3 Kaydet".
2. Hata = neden + çözüm: "Parola yanlış (AES doğrulaması başarısız). Çözüm: Anahtar Bulucu'yu çalıştır."
3. Kaydetme sonrası görünür kazanım: "Kaydedildi · yedek alındı · doğrulandı ✓".

---

## Kaynaklar

- [skills.sh](https://www.skills.sh/) · [API belgesi](https://www.skills.sh/docs/api) · [vercel-labs/skills](https://github.com/vercel-labs/skills)
- [ayghri/i-have-adhd](https://github.com/ayghri/i-have-adhd) — `skills/i-have-adhd/SKILL.md`
