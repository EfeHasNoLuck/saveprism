import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { InfoTip } from "@/components/ui/info-tip";
import { cn } from "@/lib/utils";
import { useFormatNumber } from "./context";
import { clampPage, pageCount, pagerLabel } from "./helpers";

/**
 * A titled card. The header line holds the title, the record count, the number of changed values
 * and the (i) help on the left, and the card's controls (search, pager, character picker) on the
 * right; the controls wrap under the title when the card is narrow.
 */
export function Section({
  title,
  count,
  changed = 0,
  info,
  aside,
  className,
  style,
  children,
}: {
  title: string;
  /** Record count next to the title ("125", or "12 / 125" while searching). */
  count?: string;
  /** Values in this card that differ from the file. */
  changed?: number;
  info?: string;
  aside?: ReactNode;
  className?: string;
  style?: CSSProperties;
  children: ReactNode;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  return (
    <section
      className={cn(
        "flex min-w-0 flex-col gap-2 rounded-md border border-border bg-card p-3 text-card-foreground",
        className,
      )}
      style={style}
    >
      <div className="flex min-h-7 flex-wrap items-center gap-x-3 gap-y-1.5">
        <div className="flex min-w-0 items-center gap-1.5">
          <h2 className="truncate text-sm leading-5 font-semibold">{title}</h2>
          {count !== undefined ? (
            <span className="shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums">
              {count}
            </span>
          ) : null}
          {changed > 0 ? (
            <span className="shrink-0 text-[11px] font-medium text-changed-foreground tabular-nums">
              {t("count.changed", { count: changed, changed: format(changed) })}
            </span>
          ) : null}
          {info ? <InfoTip text={info} /> : null}
        </div>
        {aside ? <div className="ml-auto flex min-w-0 items-center gap-1.5">{aside}</div> : null}
      </div>
      {children}
    </section>
  );
}

/** The quiet line of a card with nothing to show. */
export function EmptyText({ children }: { children: ReactNode }) {
  return <p className="py-1 text-xs text-muted-foreground">{children}</p>;
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
    <div className="relative w-48 min-w-0 shrink">
      <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
      <input
        type="search"
        value={value}
        aria-label={placeholder}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="h-7 w-full rounded-sm border border-input/70 bg-field pr-2 pl-7 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
      />
    </div>
  );
}

const PAGER_BUTTON =
  "inline-flex size-6 items-center justify-center rounded-sm text-foreground outline-none hover:bg-surface-container focus-visible:ring-2 focus-visible:ring-ring/60 disabled:pointer-events-none disabled:text-muted-foreground";

/** "‹ 1 / 3 ›" */
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
    <div className="flex shrink-0 items-center">
      <button
        type="button"
        aria-label={t("pager.previous")}
        title={t("pager.previous")}
        className={PAGER_BUTTON}
        disabled={current <= 0}
        onClick={() => onPage(current - 1)}
      >
        <ChevronLeft className="size-3.5" />
      </button>
      <span className="min-w-11 text-center font-mono text-[11px] text-muted-foreground tabular-nums">
        {pagerLabel(current, total, size)}
      </span>
      <button
        type="button"
        aria-label={t("pager.next")}
        title={t("pager.next")}
        className={PAGER_BUTTON}
        disabled={current >= last}
        onClick={() => onPage(current + 1)}
      >
        <ChevronRight className="size-3.5" />
      </button>
    </div>
  );
}

/**
 * Entries in `columns` columns, filled top to bottom and then left to right, so numbered entries
 * read down a column like a list.
 */
export function DenseGrid({
  columns,
  count,
  className,
  children,
}: {
  columns: number;
  /** Number of children (sets the number of rows). */
  count: number;
  className?: string;
  children: ReactNode;
}) {
  const rows = Math.max(1, Math.ceil(count / Math.max(1, columns)));
  return (
    <div
      className={cn("grid grid-flow-col gap-x-3 gap-y-0.5", className)}
      style={{
        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
        gridTemplateRows: `repeat(${rows}, auto)`,
      }}
    >
      {children}
    </div>
  );
}
