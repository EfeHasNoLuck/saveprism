# Taslak — RPGSaveEditor'e iş birliği mesajı

> **Ne zaman:** NOW.md karar #3 "evet" ise. **Nereye:** github.com/truongthang2211/RPGSaveEditor → Discussions (yoksa Issues).
> **Gönderen:** sen (Claude mesajı senin adına göndermez). ⏱ 5 dk.

---

**Title:** Collaboration idea: a shared, engine-agnostic save codec library?

Hi! First of all, thank you for RPGSaveEditor — the lossless Marshal/pickle writers, the search syntax and the save comparison feature are excellent.

I'm planning an MIT-licensed, cross-platform (Windows/macOS/Linux) save editor that focuses on the engines RPGSaveEditor doesn't cover today: Unreal GVAS, encrypted Easy Save 3, Unity PlayerPrefs/BinaryFormatter and Godot. Before building, I wanted to ask you directly:

1. Would you be open to collaborating on a shared, engine-agnostic codec library (decode → tree → lossless re-encode) that both apps could use?
2. If we adapt parts of your Apache-2.0 format code (for example the pickle writer), we will keep your copyright headers and NOTICE, mark our changes, and credit RPGSaveEditor prominently in the README and About screen. Is there anything else you'd like us to do?

No pressure either way — we're also happy to simply point RPG Maker users to RPGSaveEditor.

Thanks!
