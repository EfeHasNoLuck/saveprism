# D1 denetimi

KRITIK/YUKSEK bulgu yok. Sahiplik, kilitli kaynak, yeni bağımlılık ve sözleşme kontrolleri temiz.

## ORTA
- apps/desktop/src-tauri/tauri.conf.json:25 · CSP connect-src'de `ws:` yok; Vite HMR (ws://localhost:1420) dev'de bozulabilir. Öneri: `devCsp` ile dev'e ekle.
- .github/ISSUE_TEMPLATE/config.yml:4 · `OWNER/REPO` yer tutucusu; repo açılınca düzeltilmeli.
- apps/desktop/src-tauri/src/lib.rs:3-12 · Tauri makroları workspace clippy lint'lerine takılabilir; `kontrol.sh` ile doğrula, gerekirse dar `#[allow]`.
- apps/desktop/src/App.tsx:24 · Sabit gri renkler ve düz İngilizce metin; M0-6/M0-7 kabulüne taşıma yazılmalı.

## DUSUK
- Cargo.toml:10 · `repository = ""` (kart "boş bırak" dedi; Cargo uyarabilir).
- deny.toml:17 · `version = 2` cargo-deny 0.18+ sürümünde uyarı verebilir.
- deny.toml:65 · allow-registry gereksiz olabilir; Tauri ağacında eksik lisans çıkabilir (MIT-0, Unicode-DFS-2016 vb.).
- CONTRIBUTING.md:35,55 · `locales/<language>/` ve `fixtures/expected/` henüz yok.
- .github/ISSUE_TEMPLATE/game_support.yml:56 · `.sav` sürüklenemez; "zip it or link" demeli.
- docs/MAP.md · yeni crate'ler ve `SP-APP-001` öneki eklenmemiş (CEO işi).
