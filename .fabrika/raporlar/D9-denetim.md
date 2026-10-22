# D9 denetimi — K-3a (denetci·sonnet, 2026-10-22)

- KRITIK: yok · YUKSEK: yok
- ORTA (düzeltildi): float düğümde sınır denetimi yoktu → `check_bounds` float da denetliyor + test · min/max i64 uçları JS'te kesinlik
  kaybı → UI'a ±(2^53−1) kırpılarak gidiyor · MAP.md'de `session/game.rs` yoktu → CEO ekledi
- DUSUK (düzeltildi): `counts`/`indexed` 10 000 ile sınırlı · data dosyaları `take(MAX+1)` ile okunuyor · göreli kayıt yolunda ad araması yok
- DUSUK (bilgi): `ValueRow.path` sözleşmeye eklendi (K-3b2 kartında yazılı)
- Kanıt: `kontrol.sh rust` 3/3, 319 test (desktop 18), `pnpm -C apps/desktop typecheck` çıkış 0 (2026-10-22)
