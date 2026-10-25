import { useEffect, useMemo, useState } from "react";
import { ArrowRight, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { compareWith, type CompareResult, type CompareRow } from "@/api/commands";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { filterCompareRows } from "./compare";
import { describeError } from "./errors";

type CompareState =
  | { status: "loading" }
  | { status: "ready"; result: CompareResult }
  | { status: "error"; message: string };

interface CompareDialogProps {
  /** The second file, picked by the player. It is opened read-only. */
  path: string;
  /** Name of the file as the picker showed it, while the result loads. */
  fileName: string;
  onClose: () => void;
  /** Shows the value in the open document (the row is in the editor). */
  onReveal: (path: string) => void;
}

/** Tiny one-sided badge ("only here" / "only there"). */
function KindBadge({ row }: { row: CompareRow }) {
  const { t } = useTranslation();
  if (row.kind === "different") return null;
  return (
    <span
      className={cn(
        "shrink-0 rounded-sm px-1 text-[0.625rem] font-semibold tracking-wide uppercase",
        row.kind === "onlyHere"
          ? "bg-surface-high text-muted-foreground"
          : "bg-changed-chip text-changed-foreground",
      )}
    >
      {t(row.kind === "onlyHere" ? "compare.onlyHere" : "compare.onlyThere")}
    </span>
  );
}

/** One side of a row: the value, or a quiet dash when this file does not have the path. */
function Side({ value }: { value: string | null }) {
  if (value === null) {
    return <span className="text-muted-foreground">—</span>;
  }
  return (
    <span className="min-w-0 truncate font-mono text-[0.6875rem] text-foreground" title={value}>
      {value}
    </span>
  );
}

/**
 * Compares the open document with another save: every value that differs, plus the entries only
 * one side has. Read-only; a row the open document has jumps to that value in the editor.
 */
export function CompareDialog({ path, fileName, onClose, onReveal }: CompareDialogProps) {
  const { t } = useTranslation();
  const [state, setState] = useState<CompareState>({ status: "loading" });
  const [query, setQuery] = useState("");

  useEffect(() => {
    let cancelled = false;
    compareWith(path)
      .then((result) => !cancelled && setState({ status: "ready", result }))
      .catch((error: unknown) => {
        if (cancelled) return;
        const { reason, fix } = describeError(error);
        setState({
          status: "error",
          message: `${t("compare.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [path, t]);

  const result = state.status === "ready" ? state.result : null;
  const rows = useMemo(
    () => (result === null ? [] : filterCompareRows(result.rows, query)),
    [query, result],
  );
  const name = result?.otherFileName ?? fileName;

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("compare.title", { name })}</DialogTitle>
          <DialogDescription>{t("compare.description")}</DialogDescription>
        </DialogHeader>

        {state.status === "loading" ? (
          <p className="py-4 text-center text-[0.8125rem] text-muted-foreground">
            {t("common.loading")}
          </p>
        ) : state.status === "error" ? (
          <p role="alert" className="py-2 text-[0.8125rem] text-destructive">
            {state.message}
          </p>
        ) : (
          <>
            <div className="flex flex-wrap items-center justify-between gap-2">
              <p className="text-[0.8125rem] text-muted-foreground tabular-nums">
                {state.result.truncated
                  ? t("compare.capped", {
                      shown: state.result.rows.length,
                      count: state.result.total,
                    })
                  : t("compare.count", { count: state.result.total })}
                {query.trim() !== "" ? ` · ${t("compare.found", { count: rows.length })}` : ""}
              </p>
              <div className="relative w-56 min-w-0 shrink">
                <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <input
                  type="search"
                  value={query}
                  aria-label={t("compare.search")}
                  placeholder={t("compare.search")}
                  onChange={(event) => setQuery(event.target.value)}
                  className="h-7 w-full rounded-sm border border-input/70 bg-field pr-2 pl-7 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
                />
              </div>
            </div>

            {state.result.rows.length === 0 ? (
              <p className="rounded-sm bg-surface-high px-3 py-4 text-center text-[0.8125rem] text-muted-foreground">
                {t("compare.empty")}
              </p>
            ) : rows.length === 0 ? (
              <p className="rounded-sm bg-surface-high px-3 py-4 text-center text-[0.8125rem] text-muted-foreground">
                {t("json.noMatches")}
              </p>
            ) : (
              <ul className="dense-scroll -mx-1 flex max-h-[55vh] flex-col gap-1 overflow-y-auto px-1">
                {rows.map((row) => {
                  const body = (
                    <>
                      <span className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 truncate font-mono text-xs" title={row.path}>
                          {row.path}
                        </span>
                        <KindBadge row={row} />
                        {row.here !== null ? (
                          <ArrowRight
                            aria-hidden="true"
                            className="ml-auto size-3 shrink-0 text-muted-foreground"
                          />
                        ) : null}
                      </span>
                      <span className="grid grid-cols-2 gap-x-3 gap-y-0.5">
                        <span className="flex min-w-0 items-center gap-1">
                          <span className="shrink-0 text-[0.625rem] font-medium tracking-wide text-muted-foreground uppercase">
                            {t("compare.here")}
                          </span>
                          <Side value={row.here} />
                        </span>
                        <span className="flex min-w-0 items-center gap-1">
                          <span className="shrink-0 text-[0.625rem] font-medium tracking-wide text-muted-foreground uppercase">
                            {t("compare.there")}
                          </span>
                          <Side value={row.there} />
                        </span>
                      </span>
                    </>
                  );
                  return (
                    <li key={row.path}>
                      {row.here !== null ? (
                        <button
                          type="button"
                          title={t("compare.goTo")}
                          onClick={() => {
                            onClose();
                            onReveal(row.path);
                          }}
                          className="flex w-full flex-col gap-0.5 rounded-sm bg-surface-high px-2.5 py-1.5 text-left outline-none hover:bg-surface-container focus-visible:ring-2 focus-visible:ring-ring/60"
                        >
                          {body}
                        </button>
                      ) : (
                        <div className="flex flex-col gap-0.5 rounded-sm bg-surface-high px-2.5 py-1.5">
                          {body}
                        </div>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  );
}
