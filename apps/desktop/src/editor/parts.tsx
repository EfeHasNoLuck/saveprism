import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Field } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useEditorContext, useFormatNumber } from "./context";
import { changeText, clampPage, pageCount, pagerLabel } from "./helpers";
import { InfoTip } from "@/components/ui/info-tip";

/**
 * A titled card. The title is 16 px semibold, an optional overline sits above it, and help goes
 * into the (i) tooltip.
 */
export function Section({
  title,
  overline,
  info,
  className,
  children,
}: {
  title: string;
  overline?: string;
  info?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section
      className={cn(
        "flex min-w-0 flex-col gap-3 rounded-xl border bg-card p-5 text-card-foreground",
        className,
      )}
    >
      <div className="flex min-w-0 flex-col gap-0.5">
        {overline ? (
          <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
            {overline}
          </span>
        ) : null}
        <div className="flex min-w-0 items-center gap-1.5">
          <h2 className="truncate text-base font-semibold">{title}</h2>
          {info ? <InfoTip text={info} /> : null}
        </div>
      </div>
      {children}
    </section>
  );
}

/**
 * "2,500 → 2,601" for a changed value, nothing otherwise. Compact rows show it next to the name,
 * so the row keeps its height; the tooltip has the full text.
 */
export function ChangeNote({ field, className }: { field: Field; className?: string }) {
  const ctx = useEditorContext();
  const format = useFormatNumber();
  const latest = ctx.fields.get(field.path) ?? field;
  if (latest.value === latest.original) return null;
  return (
    <span
      className={cn("min-w-0 truncate text-xs text-changed-foreground tabular-nums", className)}
      title={changeText(latest.kind, latest.original, latest.value, format, Infinity)}
    >
      {changeText(latest.kind, latest.original, latest.value, format)}
    </span>
  );
}

/** The quiet line of a list or table with nothing to show. */
export function EmptyText({ children }: { children: ReactNode }) {
  return <p className="py-2 text-[13px] text-muted-foreground">{children}</p>;
}

export function SearchBox({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
}) {
  return (
    <div className="relative">
      <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        type="search"
        value={value}
        aria-label={placeholder}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="h-8 w-full rounded-md border bg-background pr-2 pl-7 text-[13px] outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      />
    </div>
  );
}

export function Pager({
  page,
  total,
  size,
  onPage,
}: {
  page: number;
  total: number;
  size: number;
  onPage: (page: number) => void;
}) {
  const { t } = useTranslation("editor");
  const current = clampPage(page, total, size);
  const last = pageCount(total, size) - 1;
  return (
    <div className="flex items-center justify-end gap-1 text-[13px] text-muted-foreground">
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("pager.previous")}
        className="text-foreground"
        disabled={current <= 0}
        onClick={() => onPage(current - 1)}
      >
        <ChevronLeft />
      </Button>
      <span className="min-w-12 text-center tabular-nums">{pagerLabel(current, total, size)}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        aria-label={t("pager.next")}
        className="text-foreground"
        disabled={current >= last}
        onClick={() => onPage(current + 1)}
      >
        <ChevronRight />
      </Button>
    </div>
  );
}
