import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { LanguageSwitcher } from "@/i18n";
import { ThemeToggle } from "@/theme";

function App() {
  const { t } = useTranslation();
  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  const [coreFailed, setCoreFailed] = useState(false);

  useEffect(() => {
    invoke<string>("core_version")
      .then(setCoreVersion)
      .catch(() => setCoreFailed(true));
  }, []);

  const coreLabel = coreFailed
    ? t("footer.coreUnavailable")
    : coreVersion
      ? t("footer.core", { version: coreVersion })
      : "";

  return (
    <div className="flex h-screen flex-col bg-background text-foreground">
      <header className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="font-semibold">SavePrism</span>
        <div className="flex items-center gap-2">
          <LanguageSwitcher />
          <ThemeToggle />
        </div>
      </header>
      <main className="flex flex-1 flex-col items-center justify-center gap-2">
        <p className="text-lg">{t("empty.title")}</p>
        <p className="text-sm text-muted-foreground">{t("empty.subtitle")}</p>
      </main>
      <footer className="border-t border-border px-4 py-1 text-xs text-muted-foreground">
        {coreLabel}
      </footer>
    </div>
  );
}

export default App;
