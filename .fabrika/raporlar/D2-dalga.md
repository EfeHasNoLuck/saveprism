# Dalga 2 raporu

- M0-6 · tamam (kod yazıldı, koşturulmadı) · apps/desktop/src/theme/{theme.ts,index.tsx,theme.test.ts}, src/components/ui/{button,dropdown-menu}.tsx · .fabrika/raporlar/M0-6.md
- M0-7 · tamam (koşturulmadı) · apps/desktop/locales/{en,tr}/common.json, src/i18n/{detect.ts,index.tsx,i18n.test.ts}, src/App.tsx · .fabrika/raporlar/M0-7.md
- M0-8 · tamam (koşturulmadı) · .github/workflows/ci.yml, .github/dependabot.yml, apps/desktop/{eslint.config.js,vitest.config.ts,package.json scripts}, kök package.json scripts · .fabrika/raporlar/M0-8.md

Denetim: tur 1'de 2 YUKSEK, bir düzeltme turuyla giderildi; tur 2 bulgu yok. Ayrıntı: .fabrika/raporlar/D2-denetim.md

Kullanıcıya/CEO'ya kalan: `kontrol.sh web` ve `hepsi` kanıtı, `git status` sahiplik kontrolü, `pnpm tauri dev` ile 3 tema kipi + dil + yeniden açılış (KAPI-2), push sonrası CI yeşil mi, react-hooks lint çıktısı (M0-6/M0-7 kodunda yeni v7 kuralları), Linux paket adları CI'da görülecek.
