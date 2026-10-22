import { ChevronLeft, ChevronRight, Search } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { clampPage, pageCount, pagerLabel } from "./helpers";
import { InfoTip } from "./ui/info-tip";

/** A titled block. The title is 16 px semibold; help goes into the (i) tooltip. */
export function Section({
  title,
  info,
  className,
  children,
}: {
  title: string;
  info?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={cn("flex min-w-0 flex-col gap-3 p-4", className)}>
      <div className="flex items-center gap-1.5">
        <h2 className="text-base font-semibold">{title}</h2>
        {info ? <InfoTip text={info} /> : null}
      </div>
      {children}
    </section>
  );
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
        disabled={current >= last}
        onClick={() => onPage(current + 1)}
      >
        <ChevronRight />
      </Button>
    </div>
  );
}
