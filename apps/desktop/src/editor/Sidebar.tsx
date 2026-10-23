import { Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useFormatNumber } from "./context";

export interface SectionLink {
  id: string;
  name: string;
  /** Number of rows in the section. */
  count: number;
  /** Number of values in the section that differ from the file. */
  changed: number;
}

/**
 * The section list on the left: a filter, then one line per section with its row count. The
 * selected section is tinted blue with a filled count; one with changes shows a violet count.
 */
export function SectionNav({
  sections,
  selected,
  onSelect,
}: {
  sections: SectionLink[];
  selected: string;
  onSelect: (id: string) => void;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const [query, setQuery] = useState("");
  const q = query.trim().toLowerCase();
  const shown = q === "" ? sections : sections.filter((s) => s.name.toLowerCase().includes(q));

  return (
    <nav
      aria-label={t("nav.label")}
      className="flex w-60 shrink-0 flex-col gap-2 overflow-hidden rounded-md border border-border bg-chrome p-2"
    >
      <div className="relative shrink-0">
        <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <input
          type="search"
          value={query}
          aria-label={t("nav.find")}
          placeholder={t("nav.find")}
          onChange={(e) => setQuery(e.target.value)}
          className="h-7 w-full rounded-sm border border-input/70 bg-card pr-2 pl-7 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:bg-card dark:focus-visible:border-ring"
        />
      </div>
      <span className="shrink-0 px-2 text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
        {t("nav.sections")}
      </span>
      <ul className="dense-scroll -mr-1 flex min-h-0 flex-col gap-0.5 overflow-y-auto pr-1" role="list">
        {shown.map((section) => {
          const active = section.id === selected;
          return (
            <li key={section.id}>
              <button
                type="button"
                aria-current={active || undefined}
                onClick={() => onSelect(section.id)}
                className={cn(
                  "flex h-8 w-full items-center gap-2 rounded-sm px-2 text-left text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                  active
                    ? "bg-primary/10 font-semibold text-primary"
                    : "text-foreground-variant hover:bg-surface-container hover:text-foreground",
                )}
              >
                <span className="min-w-0 flex-1 truncate" title={section.name}>
                  {section.name}
                </span>
                {section.changed > 0 ? (
                  <span
                    className="flex shrink-0 items-center gap-1 font-mono text-[11px] font-medium text-changed-foreground tabular-nums"
                    title={t("table.changed", { count: section.changed, changed: format(section.changed) })}
                  >
                    <span className="size-1.5 rounded-full bg-changed" aria-hidden="true" />
                    {format(section.changed)}
                  </span>
                ) : null}
                <span
                  className={cn(
                    "shrink-0 rounded-sm px-1.5 font-mono text-[11px] leading-[18px] tabular-nums",
                    active
                      ? "bg-primary font-semibold text-primary-foreground"
                      : "bg-surface-high text-muted-foreground",
                  )}
                >
                  {format(section.count)}
                </span>
              </button>
            </li>
          );
        })}
        {shown.length === 0 ? (
          <li className="px-2 py-1 text-xs text-muted-foreground">{t("search.empty")}</li>
        ) : null}
      </ul>
    </nav>
  );
}
