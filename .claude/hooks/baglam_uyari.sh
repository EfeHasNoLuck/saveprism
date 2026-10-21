#!/usr/bin/env bash
# Baglam ve kota esigini makineyle hatirlatir (UserPromptSubmit + PostToolUse[Agent|Task]).
# Status line her yenilemede .fabrika/durum/baglam-<oturum>.txt dosyasina "ctx=57 h5=40 sid=<oturum>" yazar.
# Bu kanca o dosyayi okur; ayni oturumda esik asildiysa modele additionalContext olarak uyari ekler.
# Esikler (tek kaynak): baglam %55 devret, %60 sinir, %75 gec; 5 saatlik kota %85.
# Bagimlilik yok, yalniz bash yerlesikleri (alt surec acmaz). Hata olursa sessizce cikar.

girdi=$(cat 2>/dev/null) || exit 0
re='"agent_id"[[:space:]]*:[[:space:]]*"[^"]+'
[[ $girdi =~ $re ]] && exit 0          # alt ajanin icindeki cagrilarda uyarma; yalniz ana oturum devreder
re='"hook_event_name"[[:space:]]*:[[:space:]]*"([A-Za-z]+)"'
olay="UserPromptSubmit"; [[ $girdi =~ $re ]] && olay="${BASH_REMATCH[1]}"
[ "$olay" = "PostToolUse" ] || olay="UserPromptSubmit"
re='"session_id"[[:space:]]*:[[:space:]]*"([A-Za-z0-9_-]+)"'
sid=""; [[ $girdi =~ $re ]] && sid="${BASH_REMATCH[1]}"
[ -z "$sid" ] && exit 0

# Status line her oturum icin ayri dosya yazar: .fabrika/durum/baglam-<oturum>.txt
dosya="${CLAUDE_PROJECT_DIR:-.}/.fabrika/durum/baglam-${sid:0:80}.txt"
[ -f "$dosya" ] || exit 0
IFS= read -r satir < "$dosya" || [ -n "$satir" ] || exit 0
ctx=""; h5=""
re='ctx=([0-9]+)'; [[ $satir =~ $re ]] && ctx="${BASH_REMATCH[1]}"
re='h5=([0-9]+)';  [[ $satir =~ $re ]] && h5="${BASH_REMATCH[1]}"

mesaj=""
if [ -n "$ctx" ]; then
  if [ "$ctx" -ge 75 ]; then
    mesaj="ANA OTURUM BAGLAMI %$ctx (geç kalındı). Yeni iş açma; yalnız devret skill'iyle HANDOFF yaz ve kullanıcıya /clear dedir."
  elif [ "$ctx" -ge 55 ]; then
    mesaj="ANA OTURUM BAGLAMI %$ctx (devir eşiği %55, sınır %60). Yeni dalga açma; koşan alt ajanları bekle, devret skill'ini uygula."
  fi
fi
if [ -n "$h5" ] && [ "$h5" -ge 85 ]; then
  mesaj="${mesaj:+$mesaj }5 SAATLİK KOTA %$h5. Yeni dalga açma; elindeki işi kapat ve HANDOFF yaz (limit işi ortasında keser)."
fi
[ -z "$mesaj" ] && exit 0
printf '{"hookSpecificOutput":{"hookEventName":"%s","additionalContext":"%s"}}\n' "$olay" "$mesaj"
exit 0
