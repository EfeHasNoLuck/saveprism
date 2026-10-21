#!/usr/bin/env bash
# Fabrika status line, bash + jq surumu (Python yoksa). statusline.py ile ayni esikler ve ayni durum dosyasi.
# Esikler: baglam <55 yesil, 55-75 sari "⚠ devret", >=75 kirmizi "⚠ DEVRET"; kota <70 yesil, 70-85 sari, >=85 kirmizi.
input=$(cat)
command -v jq >/dev/null 2>&1 || { echo "status line: jq yok (statusline.py kullan ya da jq kur)"; exit 0; }
j() { printf '%s' "$input" | jq -r "$1 // empty" 2>/dev/null; }

MODEL=$(j '.model.display_name'); EFOR=$(j '.effort.level')
DIR=$(j '.workspace.current_dir'); [ -z "$DIR" ] && DIR=$(pwd)
PROJE=$(j '.workspace.project_dir'); [ -z "$PROJE" ] && PROJE="${CLAUDE_PROJECT_DIR:-$DIR}"
PCT=$(j '.context_window.used_percentage' | cut -d. -f1); PCT=${PCT:-0}
H5=$(j '.rate_limits.five_hour.used_percentage' | cut -d. -f1)
W7=$(j '.rate_limits.seven_day.used_percentage' | cut -d. -f1)
SID=$(j '.session_id')

G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; D=$'\033[2m'; N=$'\033[0m'
if [ "$PCT" -lt 55 ]; then C=$G; U=""; elif [ "$PCT" -lt 75 ]; then C=$Y; U=" ⚠ devret"; else C=$R; U=" ⚠ DEVRET"; fi
F=$((PCT / 10)); [ "$F" -gt 10 ] && F=10; BAR=""
for i in $(seq 1 10); do if [ "$i" -le "$F" ]; then BAR="${BAR}█"; else BAR="${BAR}░"; fi; done

BR=$(git -C "$DIR" branch --show-current 2>/dev/null)
LINE="◆ ${MODEL:-?}${EFOR:+ · $EFOR} ${D}│${N} ▸ $(basename "$DIR")${BR:+ ⎇ $BR} ${D}│${N} ${C}${BAR} ${PCT}%${U}${N}"
kota() { [ -z "$2" ] && return; if [ "$2" -lt 70 ]; then K=$G; W=""; elif [ "$2" -lt 85 ]; then K=$Y; W=""; else K=$R; W=" ⚠ kota"; fi
  LINE="$LINE ${D}│${N} ${K}$1 $2%${W}${N}"; }
kota 5s "$H5"; kota 7g "$W7"
echo "$LINE"

SID=$(printf '%s' "$SID" | tr -cd 'A-Za-z0-9_-' | cut -c1-80)
if [ -n "$SID" ] && [ -d "$PROJE/.fabrika" ]; then
  mkdir -p "$PROJE/.fabrika/durum" 2>/dev/null && printf 'ctx=%s h5=%s sid=%s\n' "$PCT" "$H5" "$SID" > "$PROJE/.fabrika/durum/baglam-$SID.txt.tmp" 2>/dev/null \
    && mv -f "$PROJE/.fabrika/durum/baglam-$SID.txt.tmp" "$PROJE/.fabrika/durum/baglam-$SID.txt" 2>/dev/null
fi
exit 0
