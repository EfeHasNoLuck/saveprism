/** Contexts and hooks of the app shell. Providers live in their own files. */
import { createContext, useContext } from "react";
import type { Settings } from "./settings";

export interface SettingsContextValue {
  settings: Settings;
  update: (patch: Partial<Settings>) => void;
}

export const SettingsContext = createContext<SettingsContextValue | null>(null);

export function useSettings(): SettingsContextValue {
  const ctx = useContext(SettingsContext);
  if (!ctx) {
    throw new Error(
      "useSettings must be used inside <SettingsProvider>. Wrap the app root in <SettingsProvider> in main.tsx.",
    );
  }
  return ctx;
}

export type EventKind = "opened" | "saved" | "error" | "info";

/** One line in the bell: something that happened in this session. */
export interface AppEvent {
  id: number;
  kind: EventKind;
  title: string;
  detail?: string;
  /** Milliseconds since 1970. */
  at: number;
}

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastItem {
  id: number;
  kind: "success" | "error" | "info";
  title: string;
  detail?: string;
  action?: ToastAction;
}

export interface NotifyInput {
  kind: EventKind;
  title: string;
  detail?: string;
  /** Also show a toast (default true). */
  toast?: boolean;
  action?: ToastAction;
  /** Keep the line out of the bell (default false). */
  skipBell?: boolean;
}

export interface NotifyContextValue {
  events: AppEvent[];
  /** Events that came in since the bell was last opened. */
  unseen: number;
  /** Marks every event as seen (the bell was opened or closed). */
  markSeen: () => void;
  toasts: ToastItem[];
  notify: (input: NotifyInput) => void;
  dismissToast: (id: number) => void;
  clearEvents: () => void;
}

export const NotifyContext = createContext<NotifyContextValue | null>(null);

export function useNotify(): NotifyContextValue {
  const ctx = useContext(NotifyContext);
  if (!ctx) {
    throw new Error(
      "useNotify must be used inside <NotifyProvider>. Wrap the app root in <NotifyProvider> in main.tsx.",
    );
  }
  return ctx;
}
