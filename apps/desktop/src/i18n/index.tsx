/**
 * Internationalisation contract. The exported names and signatures below are
 * stable: `main.tsx` calls `initI18n()` before first render and `App.tsx`
 * renders `LanguageSwitcher`.
 */
import i18next from "i18next";
import { Check, ChevronDown } from "lucide-react";
import { initReactI18next, useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import en from "../../locales/en/common.json";
import tr from "../../locales/tr/common.json";
import {
  detectLanguage,
  isLanguage,
  STORAGE_KEY,
  SUPPORTED_LANGUAGES,
  type Language,
} from "./detect";
import { LanguageFlag } from "./flags";
import { languageName, languageShortCode } from "./language";

export { SUPPORTED_LANGUAGES };
export type { Language };

const resources = {
  en: { common: en },
  tr: { common: tr },
} as const;

function readStoredLanguage(): string | null {
  try {
    return window.localStorage.getItem(STORAGE_KEY);
  } catch {
    // Storage can be unavailable (privacy settings); fall back to detection.
    return null;
  }
}

function writeStoredLanguage(lang: Language): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // Storage unavailable: the choice lasts for this session only.
  }
}

function applyHtmlLang(lang: Language): void {
  if (typeof document !== "undefined") {
    document.documentElement.lang = lang;
  }
}

let initialised = false;

/** Initialises translations and the active language. Call once before first render. */
export function initI18n(): void {
  if (initialised) {
    return;
  }
  initialised = true;

  const lng = detectLanguage({ stored: readStoredLanguage() });

  // Resources are bundled, so initialisation is synchronous.
  void i18next.use(initReactI18next).init({
    resources,
    lng,
    fallbackLng: "en",
    supportedLngs: [...SUPPORTED_LANGUAGES],
    defaultNS: "common",
    ns: ["common"],
    interpolation: { escapeValue: false },
    initAsync: false,
  });

  applyHtmlLang(lng);

  // Registered after init so the detected language is not written to storage;
  // only an explicit change by the user is saved.
  i18next.on("languageChanged", (next: string) => {
    if (isLanguage(next)) {
      writeStoredLanguage(next);
      applyHtmlLang(next);
    }
  });
}

/** Language menu: a round flag, the short code and a chevron; the menu shows each language's own name. */
export function LanguageSwitcher() {
  const { t, i18n } = useTranslation();
  const active = i18n.resolvedLanguage;
  const current: Language = isLanguage(active) ? active : "en";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          aria-label={t("language.label")}
          title={t("language.label")}
          className="gap-1 px-1.5 text-foreground-variant"
        >
          <LanguageFlag language={current} className="size-4" />
          <span className="font-mono text-[0.6875rem] font-semibold uppercase">
            {languageShortCode(current)}
          </span>
          <ChevronDown className="size-3" aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel>{t("language.label")}</DropdownMenuLabel>
        <DropdownMenuSeparator />
        {SUPPORTED_LANGUAGES.map((lang) => (
          <DropdownMenuItem
            key={lang}
            className="gap-2.5"
            onSelect={() => {
              void i18n.changeLanguage(lang);
            }}
          >
            <LanguageFlag language={lang} className="size-[18px]" />
            <span>{languageName(lang)}</span>
            {lang === current ? (
              <Check className="ml-auto text-primary" aria-hidden="true" />
            ) : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
