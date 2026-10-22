import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Entry } from "@/api/commands";
import { FieldEditor } from "./FieldEditor";
import { filterEntries, pageItems } from "./helpers";
import { EmptyText, Pager, SearchBox, Section } from "./parts";

const PAGE_SIZE = 50;

export type EntryKind = "item" | "weapon" | "armor" | "variable" | "switch";

interface Props {
  title: string;
  info?: string;
  kind: EntryKind;
  entries: Entry[];
  /** Page the list by 50 (Variables, Switches). */
  paged?: boolean;
  className?: string;
}

/** Searchable list of "name  #id  editor" rows; a plain line when the save has none. */
export function EntryList({ title, info, kind, entries, paged = false, className }: Props) {
  const { t } = useTranslation("editor");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const filtered = useMemo(() => filterEntries(entries, query), [entries, query]);
  const rows = paged ? pageItems(filtered, page, PAGE_SIZE) : filtered;

  return (
    <Section title={title} info={info} className={className}>
      {entries.length === 0 ? null : (
        <SearchBox
          value={query}
          onChange={(v) => {
            setQuery(v);
            setPage(0);
          }}
          placeholder={t("search.list")}
        />
      )}
      <div className={paged ? "flex flex-col" : "flex max-h-80 flex-col overflow-y-auto"}>
        {entries.length === 0 ? (
          <EmptyText>{t(`empty.${kind}`)}</EmptyText>
        ) : rows.length === 0 ? (
          <EmptyText>{t("search.empty")}</EmptyText>
        ) : (
          rows.map((entry) => {
            const name = entry.name ?? t(`fallback.${kind}`, { id: entry.id });
            const isBool = entry.field.kind === "bool";
            return (
              <div
                key={entry.id}
                className="flex items-center gap-2 border-b py-1.5 last:border-b-0"
              >
                <div className="flex min-w-0 flex-1 items-baseline gap-1.5">
                  <span className="truncate text-[13px]" title={name}>
                    {name}
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                    #{entry.id}
                  </span>
                </div>
                <div className={isBool ? "shrink-0" : "w-44 shrink-0"}>
                  <FieldEditor field={entry.field} label={name} floatingHint />
                </div>
              </div>
            );
          })
        )}
      </div>
      {paged && filtered.length > PAGE_SIZE ? (
        <Pager page={page} total={filtered.length} size={PAGE_SIZE} onPage={setPage} />
      ) : null}
    </Section>
  );
}
