import "./index.css";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { applyStoredTheme, ThemeProvider } from "@/theme";
import { initI18n } from "@/i18n";

applyStoredTheme();
initI18n();

const container = document.getElementById("root");
if (!container) {
  throw new Error(
    "SavePrism could not start: the #root element is missing from index.html. Restore the <div id=\"root\"></div> element.",
  );
}

createRoot(container).render(
  <StrictMode>
    <ThemeProvider>
      <App />
    </ThemeProvider>
  </StrictMode>,
);
