#!/usr/bin/env bash
# PreToolUse korumasi (Bash | PowerShell | Edit | Write). Fabrika kurali: "kural talimatla degil makineyle zorlanir".
# exit 0 = izin, exit 2 = engelle (stderr Claude'a gider ve neden engellendigini okur).
# Bagimlilik yok (jq yok). Hiz icin yalniz bash yerlesikleri: her arac cagrisinda calisir, alt surec acmaz.
# Windows'ta Git Bash, macOS'ta bash 3.2+ ile calisir. Kendi hatasi isi kilitlemez (fail-open).
# SINIR: Bu bir korkuluktur, kum havuzu degildir. Metin kaliplariyla yakalanabilen kazalari durdurur; asil geri donus
# noktasi git commit'leridir. Gecici kapatma (yalniz kullanici, bilerek): KORUMA=kapali claude

[ "${KORUMA:-acik}" = "kapali" ] && exit 0
girdi=$(cat 2>/dev/null) || exit 0
[ -z "$girdi" ] && exit 0
shopt -s nocasematch

engelle() {
  printf 'KORUMA: islem calistirilmadi. %s\n' "$1" >&2
  printf 'Bu bir proje kuralidir (.claude/hooks/koruma.sh). Gerekliyse kullaniciya sor; kullanici kendi terminalinde yapar.\n' >&2
  exit 2
}

# JSON'daki ilk "ad":"deger" alaninin degeri (kacislar korunur) -> $deger
alan() {
  deger=""
  local re="\"$1\"[[:space:]]*:[[:space:]]*\"(([^\"\\\\]|\\\\.)*)\""
  [[ $girdi =~ $re ]] && deger="${BASH_REMATCH[1]}"
}

# Yol normallestirme: ters bolu -> /, cift / -> tek, /c/... -> c:/..., "." ve ".." bilesenleri cozulur -> $yol
yol_norm() {
  yol="${1//\\//}"
  while [[ $yol == *//* ]]; do yol="${yol//\/\//\/}"; done
  local re='^/([a-z])/(.*)$'
  [[ $yol =~ $re ]] && yol="${BASH_REMATCH[1]}:/${BASH_REMATCH[2]}"
  if [[ $yol == *"/."* || $yol == "."* ]]; then
    local IFS='/' i
    local -a parca out
    read -ra parca <<< "$yol"
    out=()
    for i in "${parca[@]}"; do
      case "$i" in
        .) ;;
        '') [ ${#out[@]} -eq 0 ] && out=("") ;;
        ..) [ ${#out[@]} -gt 1 ] && out=("${out[@]:0:${#out[@]}-1}") ;;
        *) out=("${out[@]}" "$i") ;;
      esac
    done
    yol="${out[*]}"
  fi
}

B='(^|[;&|(]|[[:space:]])'   # komut basi ya da ayiricidan sonra
S='[[:space:]]'
NS='[^[:space:]]'

alan tool_name; arac="$deger"

case "$arac" in
  Bash|PowerShell)
    alan command; k="$deger"
    # JSON kacislarini coz (\n ve \t komut ayiricidir), sonra tirnaklari at: "git" reset, rm "-rf" atlatamasin.
    k="${k//\\n/ ; }"; k="${k//\\t/ }"; k="${k//\\\"/\"}"
    k="${k//\"/}"; k="${k//\'/}"; k="${k//\`/}"
    # git: tirnakli/yol onekli cagri ve araya giren genel secenekler (--no-pager, -c x=y, -C dizin) dahil
    G="${B}(${NS}*[/\\])?git(\.exe)?((${S}+(-c|-C|--git-dir|--work-tree)${S}+${NS}+)|(${S}+-${NS}+))*${S}+"
    # --- geri alinamaz git ---
    re="${G}reset${S}+(${NS}+${S}+)*--hard";                     [[ $k =~ $re ]] && engelle 'git reset --hard commit edilmemis isi yok eder. Once git status/diff, sonra git stash ya da yeni commit.'
    re="${G}clean${S}+(${NS}+${S}+)*-[a-z]*f";                    [[ $k =~ $re ]] && engelle 'git clean -f izlenmeyen dosyalari kalici siler.'
    re="${G}push${S}+(${NS}+${S}+)*(--force(${S}|$)|-f(${S}|$)|--force-if-includes)"; [[ $k =~ $re ]] && engelle 'git push --force uzaktaki gecmisi ezer. Gerekirse --force-with-lease ile kullanici yapar.'
    re="${G}restore(${S}|$)"
    if [[ $k =~ $re ]]; then
      re2="--staged"; re3="--worktree|${S}-w(${S}|$)"
      { [[ ! $k =~ $re2 ]] || [[ $k =~ $re3 ]]; } && engelle 'git restore commit edilmemis degisikligi atar. Yalniz "git restore --staged" serbest; dosyayi duzeltmesi icin kodcuya geri ver ya da kullaniciya sor.'
    fi
    re="${G}checkout${S}+(${NS}+${S}+)*(--(${S}|$)|\.(${S}|$|;|&))"; [[ $k =~ $re ]] && engelle 'git checkout -- <dosya> / checkout . commit edilmemis degisikligi atar.'
    re="${G}stash${S}+(drop|clear)";                              [[ $k =~ $re ]] && engelle 'git stash drop/clear saklanan isi siler.'
    re="${G}worktree${S}+remove${S}+(${NS}+${S}+)*(-f|--force)";  [[ $k =~ $re ]] && engelle 'git worktree remove --force kaydedilmemis isi siler.'
    shopt -u nocasematch   # -D ile -d farkli: buyuk/kucuk harf duyarli
    re="${G}branch${S}+(${NS}+${S}+)*(-D|--delete${S}+--force)";  [[ $k =~ $re ]] && engelle 'git branch -D birlestirilmemis dali siler.'
    shopt -s nocasematch
    # --- ozyinelemeli / toplu silme (PowerShell kisaltmalari dahil: -Recur, -r) ---
    re="${B}rm${S}+(${NS}+${S}+)*(-[a-z]*r[a-z]*|--recursive)(${S}|$)"; [[ $k =~ $re ]] && engelle 'Ozyinelemeli silme (rm -r) yasak. Tek dosya gerekiyorsa adini ver; klasoru kullanici siler.'
    re="${B}(remove-item|ri|rm|del|erase|rmdir|rd)${S}.*${S}-r[a-z]*(${S}|$)"; [[ $k =~ $re ]] && engelle 'Remove-Item -Recurse yasak.'
    re="${B}(rd|rmdir|del|erase)${S}+(${NS}+${S}+)*/s(${S}|$)";   [[ $k =~ $re ]] && engelle 'rd/rmdir/del /s toplu silme yasak.'
    re="${B}find${S}.*(-delete|-exec${S}+rm)";                    [[ $k =~ $re ]] && engelle 'find ile toplu silme yasak.'
    re='shutil\.rmtree|os\.removedirs|remove-item.*-recurse';     [[ $k =~ $re ]] && engelle 'Betik icinden toplu silme yasak.'
    re="${B}(format(\.com)?${S}+[a-z]:|diskpart|mkfs|dd${S}+if=)"; [[ $k =~ $re ]] && engelle 'Disk bicimlendirme/yazma komutu yasak.'
    # --- dis motorlara ve kendine tehlikeli bayrak ---
    re='dangerously-bypass-approvals-and-sandbox|danger-full-access'; [[ $k =~ $re ]] && engelle 'Codex guvenlik kutusunu kapatan bayrak yasak. -s read-only ya da -s workspace-write kullan.'
    re="${B}(agy|claude)(\.exe)?${S}.*--dangerously-skip-permissions"; [[ $k =~ $re ]] && engelle 'Baska bir ajani onaysiz kipte baslatmak yasak. AFK kipini yalniz kullanici kendi terminalinden acar.'
    # --- korunan dosyalar: kanca, ayar, arac (kural kendini korur) ve SaveLens kilitli kaynaklari ---
    KORUNAN='(\.claude[/\\]+(hooks|araclar|settings(\.local)?\.json)|koruma\.sh|baglam_uyari\.sh|kontrol\.sh)'
    KILITLI='(fixtures[/\\]+samples([/\\]|[[:space:];&|)]|$)|(^|[[:space:]/\\])license([[:space:];&|)]|$))'
    YAZAN="${B}(rm|del|erase|ri|remove-item|mv|move|move-item|cp|copy|copy-item|tee|set-content|out-file|add-content|new-item|chmod|truncate|ln|python3?|py|node|perl|ruby|powershell|pwsh|cmd|sed${S}+-i)(\.exe)?(${S}|$)"
    re=">{1,2}${S}*(${NS}*[/\\])?${KORUNAN}";                     [[ $k =~ $re ]] && engelle 'Kanca, ayar ve arac dosyalari (.claude/hooks, .claude/araclar, .claude/settings.json) kilitlidir.'
    [[ $k =~ $KORUNAN ]] && [[ $k =~ $YAZAN ]] && engelle 'Kanca, ayar ve arac dosyalari (.claude/hooks, .claude/araclar, .claude/settings.json) kilitlidir; kullanici degistirir.'
    re=">{1,2}${S}*${NS}*${KILITLI}";                             [[ $k =~ $re ]] && engelle 'fixtures/samples (kullanicinin gercek kayitlari) ve LICENSE kilitli kaynaktir; kabuk yonlendirmesiyle de yazilmaz.'
    [[ $k =~ $KILITLI ]] && [[ $k =~ $YAZAN ]] && engelle 'fixtures/samples ve LICENSE kilitli kaynaktir; tasima/kopyalama/silme/betikle yazma kullanicinin isidir.'
    exit 0
    ;;
  Edit|Write|MultiEdit|NotebookEdit)
    alan file_path; hedef="$deger"
    [ -z "$hedef" ] && { alan notebook_path; hedef="$deger"; }
    [ -z "$hedef" ] && exit 0
    yol_norm "$hedef"; h="$yol"
    # Kilit kontrolu proje kokune GORELI yol uzerinden yapilir (proje D:/Temp/Oyun gibi bir yerde olabilir).
    rel=""
    if [ -n "$CLAUDE_PROJECT_DIR" ]; then
      yol_norm "$CLAUDE_PROJECT_DIR"; p="${yol%/}"
      case "$h" in
        "$p"/*) rel="${h:${#p}+1}" ;;   # uzunlukla kes: buyuk/kucuk harf farki onek silmeyi bozmasin
        *)
          # --- proje disina yazma (istisnalar: plan kipi dosyalari ve Claude'un gecici klasoru) ---
          yol_norm "${HOME:-/yok}"; ev="${yol%/}"
          case "$h" in
            "$ev"/.claude/plans/*) exit 0 ;;
            "$ev"/.claude/projects/*/memory/*) exit 0 ;;   # Claude Code kalici bellek dosyalari
            */appdata/local/temp/claude/*|/tmp/claude*|/private/tmp/claude*|*/t/claude/*) exit 0 ;;
            *) engelle "Proje disina yazma yasak: $hedef. Gecici dosyalar icin .fabrika/gecici/ kullan.";;
          esac ;;
      esac
    fi
    # --- kilitli kaynaklar (SaveLens: gercek kayit ornekleri bayt-kesin kalir; tek sahip kullanicidir) ---
    if [ -n "$rel" ]; then
      case "$rel" in
        fixtures/samples/*) engelle "fixtures/samples kullanicinin gercek kayit dosyalaridir (bayt-kesin, round-trip testlerinin dayanagi); ajan yazmaz, kullanici koyar: $hedef";;
        license) engelle "LICENSE kilitlidir; lisans degisikligi kullanicinin kararidir: $hedef";;
        target/*|*/target/*|node_modules/*|*/node_modules/*|dist/*|*/dist/*) engelle "Uretim klasoru (target/node_modules/dist) elle yazilmaz: $hedef";;
        .claude/settings.json|.claude/settings.local.json|.claude/hooks/*|.claude/araclar/*) engelle ".claude/settings*.json, .claude/hooks/ ve .claude/araclar/ kilitlidir (ayari kullanici kendi editorunde degistirir): $hedef";;
      esac
    else
      case "$h" in
        */.claude/settings.json|*/.claude/settings.local.json|*/.claude/hooks/*|*/.claude/araclar/*) engelle ".claude/settings*.json, .claude/hooks/ ve .claude/araclar/ kilitlidir: $hedef";;
      esac
    fi
    exit 0
    ;;
esac
exit 0
