# D2 denetim

## Tur 1 (M0-6, M0-7, M0-8)
- YUKSEK | apps/desktop/src/i18n/index.tsx:97 | `initImmediate` i18next v26'da yok, `initAsync` olmalı (TS2353 riski). -> düzeltildi
- YUKSEK | apps/desktop/src/theme/index.tsx:107 | effect gövdesinde senkron setState (react-hooks/set-state-in-effect). -> düzeltildi
- DUSUK | apps/desktop/vitest.config.ts:1 | hiçbir tsconfig include listesinde yok; zararsız, M0-8 sahipliğinde değil.
- DUSUK | apps/desktop/src/i18n/i18n.test.ts:3 | resolveJsonModule zaten açık; eylem yok.

## Tur 2 (düzeltme sonrası)
Bulgu yok. Küçük DUSUK not: "light/dark" seçiliyken systemDark bayatlar, "system"e dönüşte setPreference yeniden okur (theme/index.tsx:125); düzeltme gerekmez.

## Denetlenmeyenler
Kod koşturulmadı (typecheck/lint/test yok). Okunmayanlar: theme.ts, theme.test.ts, dependabot.yml, locale JSON, index.css.
