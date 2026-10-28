import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyStoredTheme, ThemeProvider } from "@/theme";
import { initI18n } from "@/i18n";
import { applyStoredAppearance } from "./shell/appearance";
import { BootGate } from "./shell/BootGate";
import { CrashBoundary } from "./shell/CrashBoundary";
import { DevCrashTrigger } from "./shell/devCrash";
import { NotifyProvider } from "./shell/NotifyProvider";
import { SettingsProvider } from "./shell/SettingsProvider";

applyStoredTheme();
applyStoredAppearance();
initI18n();

const container = document.getElementById("root");
if (!container) {
  throw new Error(
    "SavePrism could not start: the #root element is missing from index.html. Restore the <div id=\"root\"></div> element.",
  );
}

createRoot(container).render(
  <StrictMode>
    <CrashBoundary>
      {import.meta.env.DEV ? <DevCrashTrigger /> : null}
      <BootGate />
      <ThemeProvider>
        <SettingsProvider>
          <NotifyProvider>
            <App />
          </NotifyProvider>
        </SettingsProvider>
      </ThemeProvider>
    </CrashBoundary>
  </StrictMode>,
);
