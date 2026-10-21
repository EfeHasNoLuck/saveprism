"""Fabrika status line (ogrenci surumu). Claude Code stdin'den oturum JSON'u verir; tek satir yazariz.

Gosterir: model · effort | klasor ⎇ dal | baglam cubugu % kullanilan/pencere | 5s kota % ⟳ | 7g kota %
Esikler (kit genelinde tek kaynak): baglam <55 yesil, 55-75 sari "⚠ devret", >=75 kirmizi "⚠ DEVRET";
kota <70 yesil, 70-85 sari, >=85 kirmizi "⚠ kota".
Ayrica .fabrika/durum/baglam-<oturum>.txt dosyasina "ctx=.. h5=.. sid=.." yazar; baglam_uyari.sh kancasi bunu okuyup
esik asilinca modele hatirlatma ekler. Token harcamaz, yerelde calisir. Hata olursa yine bir satir yazar.
"""
import json
import os
import subprocess
import sys
import time

try:
    sys.stdout.reconfigure(encoding="utf-8")
except Exception:
    pass

YESIL, SARI, KIRMIZI, SOLUK, SIFIR = "\033[32m", "\033[33m", "\033[31m", "\033[2m", "\033[0m"


def al(d, *yol):
    for k in yol:
        if not isinstance(d, dict):
            return None
        d = d.get(k)
    return d


def sayi(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def kisalt(n):
    if n is None:
        return "?"
    return f"{n / 1_000_000:.1f}M".replace(".0M", "M") if n >= 1_000_000 else f"{round(n / 1000)}K"


def kalan(epoch):
    e = sayi(epoch)
    if not e:
        return ""
    if e > 1e12:  # milisaniye gelirse
        e /= 1000
    s = int(e - time.time())
    if s <= 0:
        return ""
    g, s = divmod(s, 86400)
    sa, s = divmod(s, 3600)
    dk = s // 60
    return f" ⟳{g}g{sa}sa" if g else (f" ⟳{sa}sa{dk:02d}dk" if sa else f" ⟳{dk}dk")


def dal(klasor):
    try:
        r = subprocess.run(["git", "-C", klasor, "branch", "--show-current"], capture_output=True,
                           text=True, timeout=1)
        return r.stdout.strip()
    except Exception:
        return ""


def main():
    try:
        v = json.loads(sys.stdin.read() or "{}")
    except Exception:
        v = {}
    model = al(v, "model", "display_name") or "?"
    efor = al(v, "effort", "level")
    klasor = al(v, "workspace", "current_dir") or al(v, "cwd") or os.getcwd()
    proje = al(v, "workspace", "project_dir") or os.environ.get("CLAUDE_PROJECT_DIR") or klasor
    ctx = sayi(al(v, "context_window", "used_percentage"))
    pencere = sayi(al(v, "context_window", "context_window_size"))
    h5 = sayi(al(v, "rate_limits", "five_hour", "used_percentage"))
    w7 = sayi(al(v, "rate_limits", "seven_day", "used_percentage"))
    h5_sif = al(v, "rate_limits", "five_hour", "resets_at")

    parca = [f"◆ {model}" + (f" · {efor}" if efor else "")]
    d = dal(klasor)
    parca.append(f"▸ {os.path.basename(os.path.normpath(klasor))}" + (f" ⎇ {d}" if d else ""))

    c = int(ctx or 0)
    renk = YESIL if c < 55 else (SARI if c < 75 else KIRMIZI)
    uyari = "" if c < 55 else (" ⚠ devret" if c < 75 else " ⚠ DEVRET")
    dolu = min(10, c // 10)
    boyut = f" {kisalt(pencere * c / 100)}/{kisalt(pencere)}" if pencere else ""
    parca.append(f"{renk}{'█' * dolu}{'░' * (10 - dolu)} {c}%{boyut}{uyari}{SIFIR}")

    def kota(ad, yuzde, ek=""):
        if yuzde is None:
            return None
        y = int(yuzde)
        r = YESIL if y < 70 else (SARI if y < 85 else KIRMIZI)
        return f"{r}{ad} {y}%{ek}{' ⚠ kota' if y >= 85 else ''}{SIFIR}"

    for p in (kota("5s", h5, kalan(h5_sif)), kota("7g", w7)):
        if p:
            parca.append(p)
    print(f" {SOLUK}│{SIFIR} ".join(parca))

    # Kancanin okuyacagi durum dosyasi: oturum basina ayri (ayni projede iki oturum birbirini ezmesin), atomik yazim.
    try:
        sid = "".join(ch for ch in str(v.get("session_id", "")) if ch.isalnum() or ch in "-_")[:80]
        if sid and os.path.isdir(os.path.join(proje, ".fabrika")):
            durum = os.path.join(proje, ".fabrika", "durum")
            os.makedirs(durum, exist_ok=True)
            hedef = os.path.join(durum, f"baglam-{sid}.txt")
            gecici = hedef + ".tmp"
            with open(gecici, "w", encoding="utf-8") as f:
                f.write(f"ctx={c} h5={int(h5) if h5 is not None else ''} sid={sid}\n")
            os.replace(gecici, hedef)
    except Exception:
        pass


if __name__ == "__main__":
    try:
        main()
    except Exception as e:  # status line asla bos kalmasin
        print(f"status line hatasi: {type(e).__name__}")
