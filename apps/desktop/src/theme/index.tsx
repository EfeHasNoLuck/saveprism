/**
 * Theme contract (filled in by M0-6). The exported names and signatures below
 * are stable: `main.tsx` calls `applyStoredTheme()` before first render, wraps
 * the app in `ThemeProvider`, and `App.tsx` renders `ThemeToggle`.
 */
import type { ReactNode } from "react";

export type ThemePreference = "light" | "dark" | "system";

/** Applies the stored theme preference to the document. Stub: does nothing. */
export function applyStoredTheme(): void {}

/** Provides theme state to the tree. Stub: renders children unchanged. */
export function ThemeProvider({ children }: { children: ReactNode }) {
  return <>{children}</>;
}

/** Control for switching the theme. Stub: renders nothing. */
export function ThemeToggle() {
  return null;
}
