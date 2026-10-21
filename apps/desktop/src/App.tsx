import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LanguageSwitcher } from "@/i18n";
import { ThemeToggle } from "@/theme";

function App() {
  const [coreVersion, setCoreVersion] = useState<string | null>(null);
  const [coreFailed, setCoreFailed] = useState(false);

  useEffect(() => {
    invoke<string>("core_version")
      .then(setCoreVersion)
      .catch(() => setCoreFailed(true));
  }, []);

  const coreLabel = coreFailed
    ? "core unavailable"
    : coreVersion
      ? `core v${coreVersion}`
      : "";

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center justify-between border-b border-gray-300 px-4 py-2">
        <span className="font-semibold">SavePrism</span>
        <div className="flex items-center gap-2">
          <LanguageSwitcher />
          <ThemeToggle />
        </div>
      </header>
      <main className="flex flex-1 flex-col items-center justify-center gap-2">
        <p className="text-lg">Drop a save file here</p>
        <p className="text-sm text-gray-500">Everything stays on your computer.</p>
      </main>
      <footer className="border-t border-gray-300 px-4 py-1 text-xs text-gray-500">
        {coreLabel}
      </footer>
    </div>
  );
}

export default App;
