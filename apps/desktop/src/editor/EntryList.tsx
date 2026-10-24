import { useMemo, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { Entry, NamedId } from "@/api/commands";
import { AddEntryButton } from "./AddEntry";
import { BulkEdit } from "./BulkEdit";
import { countPending, useEditorContext, useFormatNumber } from "./context";
import { FieldCell } from "./FieldEditor";
import { useGridColumns } from "./grid";
import { bulkTargets, filterEntries, pageCount, pageItems, ROWS_PER_PAGE } from "./helpers";
import { DenseGrid, EmptyText, Pager, SearchBox, Section } from "./parts";

/** Lists with more entries than this get a search box. */
const SEARCH_FROM = 10;

export type EntryKind = "item" | "weapon" | "armor" | "variable" | "switch";

export interface EntryListProps {
  title: string;
  info?: string;
  kind: EntryKind;
  entries: Entry[];
  /** The list takes new entries ("Add" in the header). */
  canAdd?: boolean;
  /** The game's entries for this list (empty without a data folder). */
  catalog?: NamedId[];
  className?: string;
  style?: CSSProperties;
}

/**
 * A card of numbered entries ("Potion #1 [12] ↶") in as many columns as fit, a page at a time.
 * The header has the record count, the number of changed entries, a search box, the pager and
 * "Set all".
 */
export function EntryList({
  title,
  info,
  kind,
  entries,
  canAdd = false,
  catalog = [],
  className,
  style,
}: EntryListProps) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending, fields, readOnly } = useEditorContext();
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
  // "Set all" changes what the search shows, on every page; it is left out of a read-only file
  // and of a list with nothing it can set (only text variables).
  const bulkKind = kind === "switch" ? "bool" : "number";
  const settable = useMemo(
    () => !readOnly && bulkTargets(entries.map((e) => e.field), bulkKind).length > 0,
    [bulkKind, entries, readOnly],
  );
  const targets = useMemo(
    () =>
      bulkTargets(
        filtered.map((e) => fields.get(e.field.path) ?? e.field),
        bulkKind,
      ),
    [bulkKind, fields, filtered],
  );
  const searching = query.trim() !== "";
  const count = searching
    ? `${format(filtered.length)} / ${format(entries.length)}`
    : format(entries.length);

  // A short list shows whole; it needs no search box.
  const search =
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
    );
  // Last in the header, so it keeps its place when the pager comes and goes.
  const bulk = settable ? (
    <BulkEdit kind={bulkKind} targets={targets} searching={searching} />
  ) : null;
  const list =
    kind === "item" ? "items" : kind === "weapon" ? "weapons" : kind === "armor" ? "armors" : null;
  const add =
    canAdd && list !== null ? (
      <AddEntryButton list={list} catalog={catalog} label={t(`addEntry.label.${kind}`)} />
    ) : null;

  return (
    <Section
      title={title}
      count={count}
      changed={changed}
      info={info}
      className={className}
      style={style}
      aside={search === null && bulk === null && add === null ? null : (
        <>
          {search}
          {add}
          {bulk}
        </>
      )}
    >
      <div ref={ref} className="min-w-0">
        {entries.length === 0 ? (
          <EmptyText>{t(`empty.${kind}`)}</EmptyText>
        ) : shown.length === 0 ? (
          <EmptyText>{t("search.empty")}</EmptyText>
        ) : (
          <DenseGrid columns={columns} count={shown.length}>
            {shown.map((entry) => {
              const name = entry.name ?? t(`fallback.${kind}`, { id: entry.id });
              const remove =
                entry.removable && !entry.field.added
                  ? { path: entry.field.path, name }
                  : undefined;
              return entry.name === null ? (
                // Without a name the fallback already carries the number ("Variable #12").
                <FieldCell key={entry.id} field={entry.field} name={name} remove={remove} />
              ) : (
                <FieldCell
                  key={entry.id}
                  field={entry.field}
                  name={entry.name}
                  id={entry.id}
                  remove={remove}
                />
              );
            })}
          </DenseGrid>
        )}
      </div>
    </Section>
  );
}
