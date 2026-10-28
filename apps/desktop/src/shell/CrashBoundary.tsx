import { getVersion } from "@tauri-apps/api/app";
import i18next from "i18next";
import { Component, useCallback, useEffect, useState, type ErrorInfo, type ReactNode } from "react";
import { dismissBootCover } from "./boot";
import {
  buildCrashReport,
  CRASH_TEXT_FALLBACK,
  describeError,
  type CrashTextKey,
} from "./crashReport";

/**
 * A string of the crash screen. It reads i18next directly instead of using a hook, and falls back to
 * English when translations are missing or failed to start, so this screen never depends on the
 * parts of the app that may be the ones that broke.
 */
function crashText(key: CrashTextKey): string {
  const fallback = CRASH_TEXT_FALLBACK[key];
  try {
    const value: unknown = i18next.t(`crash.${key}`, { defaultValue: fallback });
    return typeof value === "string" && value !== "" ? value : fallback;
  } catch {
    return fallback;
  }
}

const BUTTON =
  "inline-flex h-8 items-center justify-center rounded-md px-3 text-[13px] font-medium outline-offset-2 focus-visible:outline-2";

/** What the player sees when the app failed to render. Colours come from the app's own tokens. */
function CrashScreen({ error, componentStack }: { error: unknown; componentStack: string }) {
  const [version, setVersion] = useState("unknown");
  const [copy, setCopy] = useState<"idle" | "copied" | "failed">("idle");
  const [report, setReport] = useState("");

  // The opening cover sits above everything until the first good render; a crash may come first.
  useEffect(() => {
    dismissBootCover();
  }, []);

  useEffect(() => {
    let cancelled = false;
    getVersion()
      .then((value) => {
        if (!cancelled) setVersion(value);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const copyReport = useCallback(async () => {
    const { name, message } = describeError(error);
    const text = buildCrashReport({ version, name, message, componentStack, date: new Date() });
    setReport(text);
    try {
      await navigator.clipboard.writeText(text);
      setCopy("copied");
    } catch {
      setCopy("failed");
    }
  }, [componentStack, error, version]);

  return (
    <div
      role="alert"
      className="fixed inset-0 z-50 flex flex-col items-center justify-center gap-4 overflow-auto bg-background p-6 text-center text-foreground"
    >
      <h1 className="text-lg font-semibold">{crashText("title")}</h1>
      <p className="max-w-md text-foreground-variant">{crashText("body")}</p>
      <div className="flex gap-2">
        <button
          type="button"
          className={`${BUTTON} border border-border bg-secondary text-secondary-foreground hover:bg-surface-high outline-ring`}
          onClick={() => void copyReport()}
        >
          {crashText("copy")}
        </button>
        <button
          type="button"
          className={`${BUTTON} bg-primary text-primary-foreground hover:opacity-90 outline-ring`}
          onClick={() => window.location.reload()}
        >
          {crashText("reload")}
        </button>
      </div>
      <p role="status" className="min-h-[18px] text-muted-foreground">
        {copy === "copied" ? crashText("copied") : copy === "failed" ? crashText("copyFailed") : ""}
      </p>
      {copy === "failed" ? (
        <textarea
          readOnly
          rows={8}
          value={report}
          aria-label={crashText("copyFailed")}
          onFocus={(event) => event.currentTarget.select()}
          className="w-full max-w-xl rounded-md border border-input bg-field p-2 text-left font-mono text-xs text-foreground"
        />
      ) : null}
    </div>
  );
}

interface State {
  failure: { error: unknown; componentStack: string } | null;
}

/**
 * Catches an error thrown while React renders anything below it and shows {@link CrashScreen}
 * instead of a blank window. Place it outside the providers so an error in a provider is caught
 * too. Render errors only: it does not listen for `error` or `unhandledrejection` events.
 */
export class CrashBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { failure: null };

  static getDerivedStateFromError(error: unknown): State {
    return { failure: { error, componentStack: "" } };
  }

  componentDidCatch(_error: unknown, info: ErrorInfo): void {
    // The screen is already up; this adds React's component stack to the report.
    this.setState((state) =>
      state.failure === null
        ? null
        : { failure: { ...state.failure, componentStack: info.componentStack ?? "" } },
    );
  }

  render(): ReactNode {
    const { failure } = this.state;
    if (failure === null) return this.props.children;
    return <CrashScreen error={failure.error} componentStack={failure.componentStack} />;
  }
}
