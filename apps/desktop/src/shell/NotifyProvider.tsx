import { useCallback, useMemo, useRef, useState, type ReactNode } from "react";
import {
  dismissToastsForTopic,
  NotifyContext,
  type AppEvent,
  type NotifyContextValue,
  type NotifyInput,
  type ToastItem,
} from "./context";

/** Most events the bell keeps. */
const MAX_EVENTS = 50;
/** How long a toast stays, in milliseconds. Errors stay longer. */
const TOAST_MS = 8000;
const TOAST_ERROR_MS = 14000;

/** Keeps this session's events (the bell) and the toasts on screen. */
export function NotifyProvider({ children }: { children: ReactNode }) {
  const [events, setEvents] = useState<AppEvent[]>([]);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  /** Highest event id the user has seen in the bell. */
  const [seenId, setSeenId] = useState(0);
  const nextId = useRef(1);

  const dismissToast = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const dismissTopic = useCallback((topic: string) => {
    setToasts((current) => dismissToastsForTopic(current, topic));
  }, []);

  const notify = useCallback(
    (input: NotifyInput) => {
      const id = nextId.current;
      nextId.current += 1;
      if (!input.skipBell) {
        const event: AppEvent = {
          id,
          kind: input.kind,
          title: input.title,
          detail: input.detail,
          at: Date.now(),
        };
        setEvents((current) => [event, ...current].slice(0, MAX_EVENTS));
      }
      if (input.toast ?? true) {
        const kind = input.kind === "saved" ? "success" : input.kind === "error" ? "error" : "info";
        const toast: ToastItem = {
          id,
          kind,
          title: input.title,
          detail: input.detail,
          action: input.action,
          topic: input.topic,
        };
        setToasts((current) => [...current, toast].slice(-4));
        window.setTimeout(() => dismissToast(id), kind === "error" ? TOAST_ERROR_MS : TOAST_MS);
      }
    },
    [dismissToast],
  );

  const clearEvents = useCallback(() => setEvents([]), []);
  const markSeen = useCallback(() => setSeenId(nextId.current - 1), []);
  const unseen = useMemo(() => events.filter((event) => event.id > seenId).length, [events, seenId]);

  const value = useMemo<NotifyContextValue>(
    () => ({ events, unseen, markSeen, toasts, notify, dismissToast, dismissTopic, clearEvents }),
    [events, unseen, markSeen, toasts, notify, dismissToast, dismissTopic, clearEvents],
  );
  return <NotifyContext.Provider value={value}>{children}</NotifyContext.Provider>;
}
