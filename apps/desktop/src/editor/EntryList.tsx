import { useMemo, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { Entry } from "@/api/commands";
import { countPending, useEditorContext, useFormatNumber } from "./context";
import { FieldCell } from "./FieldEditor";
import { useGridColumns } from "./grid";
import { filterEntries, pageCount, pageItems, ROWS_PER_PAGE } from "./helpers";
import { DenseGrid, EmptyText, Pager, SearchBox, Section } from "./parts";

/** Lists with more entries than this get a search box. */
const SEARCH_FROM = 10;

export type EntryKind = "item" | "weapon" | "armor" | "variable" | "switch";

export interface EntryListProps {
  title: string;
  info?: string;
  kind: EntryKind;
  entries: Entry[];
  className?: string;
  style?: CSSProperties;
}

/**
 * A card of numbered entries ("Potion #1 [12] ↶") in as many columns as fit, a page at a time.
 * The header has the record count, the number of changed entries, a search box and the pager.
 */
export function EntryList({ title, info, kind, entries, className, style }: EntryListProps) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending } = useEditorContext();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const { ref, columns } = useGridColumns();

  const size = columns * ROWS_PER_PAGE;
  const filtered = useMemo(() => filterEntries(entries, query), [entries, query]);
  const shown = pageItems(filtered, page, size);
  const changed = useMemo(
    () =>
      countPending(
        pending,
        entries.map((e) => e.field.path),
      ),
    [entries, pending],
  );
  const searching = query.trim() !== "";
  const count = searching
    ? `${format(filtered.length)} / ${format(entries.length)}`
    : format(entries.length);

  return (
    <Section
      title={title}
      count={count}
      changed={changed}
      info={info}
      className={className}
      style={style}
      aside={
        // A short list shows whole; it needs no search box.
        entries.length <= SEARCH_FROM ? null : (
          <>
            <SearchBox
              value={query}
              onChange={(v) => {
                setQuery(v);
                setPage(0);
              }}
              placeholder={t("search.list")}
            />
            {pageCount(filtered.length, size) > 1 ? (
              <Pager page={page} total={filtered.length} size={size} onPage={setPage} />
            ) : null}
          </>
        )
      }
    >
      <div ref={ref} className="min-w-0">
        {entries.length === 0 ? (
          <EmptyText>{t(`empty.${kind}`)}</EmptyText>
        ) : shown.length === 0 ? (
          <EmptyText>{t("search.empty")}</EmptyText>
        ) : (
          <DenseGrid columns={columns} count={shown.length}>
            {shown.map((entry) =>
              entry.name === null ? (
                // Without a name the fallback already carries the number ("Variable #12").
                <FieldCell
                  key={entry.id}
                  field={entry.field}
                  name={t(`fallback.${kind}`, { id: entry.id })}
                />
              ) : (
                <FieldCell key={entry.id} field={entry.field} name={entry.name} id={entry.id} />
              ),
            )}
          </DenseGrid>
        )}
      </div>
    </Section>
  );
}
