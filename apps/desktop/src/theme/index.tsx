/**
 * Theme system. The exported names and signatures are stable:
 * `main.tsx` calls `applyStoredTheme()` before first render, wraps the app in
 * `ThemeProvider`, and `App.tsx` renders `ThemeToggle`.
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { Monitor, Moon, Sun, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  parseStoredPreference,
  resolveTheme,
  STORAGE_KEY,
  type ResolvedTheme,
  type ThemePreference,
} from "./theme";

export type { ThemePreference, ResolvedTheme } from "./theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

function getSystemPrefersDark(): boolean {
  try {
    return window.matchMedia(DARK_QUERY).matches;
  } catch {
    return false;
  }
}

function readStoredPreference(): ThemePreference {
  try {
    return parseStoredPreference(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    // Storage is unavailable (e.g. blocked by policy): fall back to system.
    return "system";
  }
}

function writeStoredPreference(pref: ThemePreference): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // Storage is unavailable: the choice still applies for this session.
  }
}

function applyToDocument(resolved: ResolvedTheme): void {
  const root = document.documentElement;
  root.classList.toggle("dark", resolved === "dark");
  // Overrides the `color-scheme: light dark` default in index.css so native
  // controls (scrollbars, form fields) follow the chosen theme.
  root.style.colorScheme = resolved;
}

/**
 * Applies the stored preference to <html> synchronously. Called before the
 * first React render so the first frame already has the right theme.
 */
export function applyStoredTheme(): void {
  const pref = readStoredPreference();
  applyToDocument(resolveTheme(pref, getSystemPrefersDark()));
}

type ThemeContextValue = {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
  setPreference: (pref: ThemePreference) => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

/** Provides theme state to the tree and keeps <html> in sync with it. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(
    readStoredPreference,
  );
  const [systemDark, setSystemDark] = useState<boolean>(getSystemPrefersDark);
  const resolvedTheme = resolveTheme(preference, systemDark);

  // Follow live OS theme changes, but only while "system" is selected.
  useEffect(() => {
    if (preference !== "system") {
      return;
    }
    let mql: MediaQueryList;
    try {
      mql = window.matchMedia(DARK_QUERY);
    } catch {
      return;
    }
    // Synchronous re-read is not needed here: the value is refreshed when the
    // preference changes to "system" (see setPreference) and at mount (lazy init).
    const onChange = (event: MediaQueryListEvent) => {
      setSystemDark(event.matches);
    };
    mql.addEventListener("change", onChange);
    return () => {
      mql.removeEventListener("change", onChange);
    };
  }, [preference]);

  useLayoutEffect(() => {
    applyToDocument(resolvedTheme);
  }, [resolvedTheme]);

  const setPreference = useCallback((pref: ThemePreference) => {
    writeStoredPreference(pref);
    if (pref === "system") {
      setSystemDark(getSystemPrefersDark());
    }
    setPreferenceState(pref);
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({ preference, resolvedTheme, setPreference }),
    [preference, resolvedTheme, setPreference],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

/** Reads the current theme preference and resolved theme. */
export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error(
      "useTheme must be used inside <ThemeProvider>. Wrap the app root in <ThemeProvider> in main.tsx.",
    );
  }
  return ctx;
}

const OPTIONS: ReadonlyArray<{
  value: ThemePreference;
  labelKey: "theme.light" | "theme.dark" | "theme.system";
  Icon: LucideIcon;
}> = [
  { value: "light", labelKey: "theme.light", Icon: Sun },
  { value: "dark", labelKey: "theme.dark", Icon: Moon },
  { value: "system", labelKey: "theme.system", Icon: Monitor },
];

function isThemePreference(value: string): value is ThemePreference {
  return value === "light" || value === "dark" || value === "system";
}

/** Theme switcher: a ghost icon button that opens a radio menu of the three modes. */
export function ThemeToggle() {
  const { t } = useTranslation();
  const { preference, resolvedTheme, setPreference } = useTheme();
  const CurrentIcon = resolvedTheme === "dark" ? Moon : Sun;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label={t("theme.toggle")}>
          <CurrentIcon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuRadioGroup
          value={preference}
          onValueChange={(value) => {
            if (isThemePreference(value)) {
              setPreference(value);
            }
          }}
        >
          {OPTIONS.map(({ value, labelKey, Icon }) => (
            <DropdownMenuRadioItem key={value} value={value}>
              <Icon />
              {t(labelKey)}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
