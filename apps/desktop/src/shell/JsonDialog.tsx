import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Copy, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

export type JsonState =
  | { status: "loading" }
  | { status: "ready"; text: string }
  | { status: "error"; message: string };

/** Height of one text line in pixels; the list draws only the lines that are in view. */
const LINE_HEIGHT = 20;
const OVERSCAN = 20;
const MAX_MATCHES = 10_000;

/** Scrolls the view so that `line` sits in its middle. */
function scrollToLine(element: HTMLElement | null, line: number) {
  if (element) {
    element.scrollTop = Math.max(0, line * LINE_HEIGHT - element.clientHeight / 2);
  }
}

interface JsonDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  state: JsonState;
}

/** Read-only pretty JSON of the open document, with search and Copy. */
export function JsonDialog({ open, onOpenChange, state }: JsonDialogProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [current, setCurrent] = useState(0);
  const [copied, setCopied] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewHeight, setViewHeight] = useState(480);
  const viewRef = useRef<HTMLDivElement | null>(null);

  const lines = useMemo(
    () => (state.status === "ready" ? state.text.split("\n") : []),
    [state],
  );

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (needle === "") return [];
    const found: number[] = [];
    for (let index = 0; index < lines.length; index += 1) {
      if ((lines[index] ?? "").toLowerCase().includes(needle)) {
        found.push(index);
        if (found.length >= MAX_MATCHES) break;
      }
    }
    return found;
  }, [lines, query]);

  // Follow the size of the viewport (the dialog can be resized with the window).
  useEffect(() => {
    const element = viewRef.current;
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setViewHeight(element.clientHeight));
    observer.observe(element);
    return () => observer.disconnect();
  }, [state.status, open]);

  // A new search shows its first match right away (Enter then goes to the second one).
  useEffect(() => {
    const first = matches[0];
    if (first !== undefined) scrollToLine(viewRef.current, first);
  }, [matches]);

  const jumpTo = (matchIndex: number) => {
    if (matches.length === 0) return;
    const wrapped = (matchIndex + matches.length) % matches.length;
    setCurrent(wrapped);
    scrollToLine(viewRef.current, matches[wrapped] ?? 0);
  };

  const activeLine = matches.length > 0 ? (matches[Math.min(current, matches.length - 1)] ?? -1) : -1;
  const matchSet = useMemo(() => new Set(matches), [matches]);

  const first = Math.max(0, Math.floor(scrollTop / LINE_HEIGHT) - OVERSCAN);
  const last = Math.min(lines.length, Math.ceil((scrollTop + viewHeight) / LINE_HEIGHT) + OVERSCAN);
  const visible = lines.slice(first, last);

  const copy = async () => {
    if (state.status !== "ready") return;
    try {
      await navigator.clipboard.writeText(state.text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="h-[80vh] max-w-4xl">
        <DialogHeader>
          <DialogTitle>{t("json.title")}</DialogTitle>
          <DialogDescription>{t("json.readOnlyNote")}</DialogDescription>
        </DialogHeader>

        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-muted-foreground" />
            <input
              type="search"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                setCurrent(0);
              }}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                jumpTo(event.shiftKey ? current - 1 : current + 1);
              }}
              placeholder={t("json.search")}
              aria-label={t("json.search")}
              className="h-9 w-full rounded-lg border border-input bg-background pr-3 pl-8 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </div>
          {query.trim() !== "" ? (
            <span className="text-sm whitespace-nowrap text-muted-foreground">
              {matches.length === 0
                ? t("json.noMatches")
                : `${Math.min(current, matches.length - 1) + 1} / ${matches.length}`}
            </span>
          ) : null}
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("json.previous")}
            disabled={matches.length === 0}
            onClick={() => jumpTo(current - 1)}
          >
            <ChevronUp />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            aria-label={t("json.next")}
            disabled={matches.length === 0}
            onClick={() => jumpTo(current + 1)}
          >
            <ChevronDown />
          </Button>
          <Button
            variant="outline"
            disabled={state.status !== "ready"}
            onClick={() => void copy()}
          >
            <Copy />
            {copied ? t("common.copied") : t("common.copy")}
          </Button>
        </div>

        <div className="min-h-0 flex-1 rounded-lg border border-border bg-background">
          {state.status === "loading" ? (
            <p className="p-4 text-sm text-muted-foreground">{t("common.loading")}</p>
          ) : state.status === "error" ? (
            <p role="alert" className="p-4 text-sm text-destructive">
              {state.message}
            </p>
          ) : (
            <div
              ref={viewRef}
              onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
              className="h-full overflow-auto font-mono text-xs"
              tabIndex={0}
            >
              <div style={{ height: lines.length * LINE_HEIGHT, position: "relative" }}>
                <div
                  style={{
                    position: "absolute",
                    top: first * LINE_HEIGHT,
                    left: 0,
                    minWidth: "100%",
                    width: "max-content",
                  }}
                >
                  {visible.map((line, offset) => {
                    const index = first + offset;
                    return (
                      <div
                        key={index}
                        style={{ height: LINE_HEIGHT, lineHeight: `${LINE_HEIGHT}px` }}
                        className={cn(
                          "px-3 whitespace-pre",
                          index === activeLine
                            ? "bg-brand/30"
                            : matchSet.has(index)
                              ? "bg-brand/10"
                              : undefined,
                        )}
                      >
                        {line}
                      </div>
                    );
                  })}
                </div>
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
