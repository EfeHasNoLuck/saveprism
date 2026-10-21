#!/usr/bin/env bash
# Derleme / test kaniti (dogrula istasyonu). "Rapor iddiadir, kanit degildir": bu komutun ciktisi kanittir.
# Kullanim:  bash .claude/araclar/kontrol.sh [hepsi|rust|web]
#   rust   Cargo.toml varsa: cargo fmt --check · cargo clippy (-D warnings) · cargo test
#   web    package.json varsa: tanimli pnpm script'leri (typecheck · lint · test)
#   hepsi  ikisi (varsayilan)
# Cikti en cok ~20 satir. Tam log: .fabrika/raporlar/kontrol-<kip>.log
# Cikis kodu: 0 = hata yok · 1 = derleme/test hatasi · 3 = calistirilamadi (arac ya da kod henuz yok)

kip="${1:-hepsi}"
cd "$(dirname "$0")/../.." || { echo "proje koku bulunamadi"; exit 3; }
mkdir -p .fabrika/raporlar
LOG=".fabrika/raporlar/kontrol-${kip}.log"
: > "$LOG"

kosulan=0; hata=0; atlanan=()

adim() { # $1 = etiket, gerisi = komut. Ciktiyi log'a yazar; basariyi/basarisizligi tek satirla bildirir.
  local etiket="$1"; shift
  printf '\n===== %s: %s\n' "$etiket" "$*" >> "$LOG"
  if "$@" >> "$LOG" 2>&1; then
    echo "  ✓ $etiket"
  else
    echo "  ✗ $etiket (cikis $?)"
    hata=$((hata + 1))
  fi
  kosulan=$((kosulan + 1))
}

rust_kontrol() {
  if [ ! -f Cargo.toml ]; then atlanan+=("rust: Cargo.toml yok (M0-5)"); return; fi
  if ! command -v cargo >/dev/null 2>&1; then atlanan+=("rust: cargo bulunamadi (M0-4: rustup kur)"); return; fi
  echo "RUST"
  adim "cargo fmt" cargo fmt --all -- --check
  adim "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
  adim "cargo test" cargo test --workspace
}

web_kontrol() {
  if [ ! -f package.json ]; then atlanan+=("web: package.json yok (M0-5)"); return; fi
  if ! command -v pnpm >/dev/null 2>&1; then atlanan+=("web: pnpm bulunamadi"); return; fi
  if [ ! -d node_modules ]; then atlanan+=("web: node_modules yok (once: pnpm install)"); return; fi
  local scriptler
  scriptler=$(node -e "const s=require('./package.json').scripts||{};process.stdout.write(Object.keys(s).join(' '))" 2>/dev/null)
  echo "WEB"
  local s bulunan=0
  for s in typecheck lint test; do
    case " $scriptler " in
      *" $s "*) adim "pnpm $s" pnpm run --silent "$s"; bulunan=1 ;;
    esac
  done
  [ $bulunan -eq 0 ] && atlanan+=("web: package.json'da typecheck/lint/test script'i yok")
}

case "$kip" in
  rust)  rust_kontrol ;;
  web)   web_kontrol ;;
  hepsi) rust_kontrol; web_kontrol ;;
  *) echo "Kullanim: bash .claude/araclar/kontrol.sh [hepsi|rust|web]"; exit 3 ;;
esac

for a in "${atlanan[@]}"; do echo "  - atlandi: $a"; done

if [ $kosulan -eq 0 ]; then
  echo "KONTROL: calistirilacak adim yok. Tam log: $LOG"
  exit 3
fi
if [ $hata -gt 0 ]; then
  echo "KONTROL: $hata/$kosulan adim basarisiz. Ilk hata satirlari (ham metin):"
  grep -E -i '^(error|warning: unused)|error(\[E[0-9]+\])?:|FAILED|failed|✗|panicked' "$LOG" | head -n 12
  echo "Tam log: $LOG"
  exit 1
fi
echo "KONTROL: $kosulan/$kosulan adim gecti. Tam log: $LOG"
exit 0
