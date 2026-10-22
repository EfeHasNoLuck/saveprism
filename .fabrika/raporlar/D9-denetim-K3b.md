# D9 denetimi — K-3b1 + K-3b2 (denetci·sonnet, 2026-10-22)

- KRITIK: yok · YUKSEK: yok · sahiplik temiz (session/mod.rs'te yalnız `mod export;`)
- Kabul edilenler: Save as / Overwrite doğru komutlar; geçersiz alan, salt-okunur ya da meşgulken kaydet kapalı; parola saklanmıyor/loglanmıyor;
  yasak etiketler yok; Recent files varsayılan kapalı; ayarlar ortada modal; switch sabit boyut; ipuçları yalnız odak/değişiklikte
- ORTA (düzeltme turu): `export_json` testleri yok → eklendi · MAP.md'de `export.rs`, `src/shell/`, `src/editor/` yok → CEO ekledi
- ORTA (kabul, not): 50 MB sınırı JSON üretildikten sonra denetleniyor; bellek belge boyutuyla sınırlı
- DUSUK (düzeltme turu): kaydedilmemiş değişiklik varken yeni dosya açma onaysızdı → onay penceresi · Save as sonrası menü adı
  "Overwrite current file"
- DUSUK (sonra): `editor/ui/switch.tsx`, `editor/ui/info-tip.tsx` ile `components/ui` ve `shell/InfoTip.tsx` tekrarı → birleştirilecek
