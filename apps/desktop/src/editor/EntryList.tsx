import { Pencil } from "lucide-react";
import {
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import type { Entry, NamedId } from "@/api/commands";
import { cn } from "@/lib/utils";
import {
  entryName,
  loadNames,
  MAX_NAME_CHARS,
  nameFor,
  saveNames,
  setOwnName,
  type OwnNameList,
  type OwnNames,
} from "@/shell/names";
import { browserStorage } from "@/shell/settings";
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
  /** The game folder own names are kept under; null or missing hides them (pasted codes). */
  namesGame?: string | null;
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
  namesGame,
  className,
  style,
}: EntryListProps) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending, fields, readOnly } = useEditorContext();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const { ref, columns } = useGridColumns();

  // Own names exist only for variables and switches, and only for a real game folder.
  const ownList: OwnNameList | null =
    kind === "variable" ? "variables" : kind === "switch" ? "switches" : null;
  const storage = useMemo(() => browserStorage(), []);
  // The stored names are read during render; edits are kept together with the game they belong
  // to, so opening another game never shows the previous game's names.
  const stored = useMemo(() => loadNames(storage, namesGame), [namesGame, storage]);
  const [saved, setSaved] = useState<{ game: string | null | undefined; names: OwnNames } | null>(
    null,
  );
  const names = saved !== null && saved.game === namesGame ? saved.names : stored;
  const [naming, setNaming] = useState<{ game: string | null | undefined; id: number } | null>(
    null,
  );
  const editing = naming !== null && naming.game === namesGame ? naming.id : null;
  const [draft, setDraft] = useState("");
  const cancelBlur = useRef(false);

  function startEdit(entry: Entry, own: string | null) {
    cancelBlur.current = false;
    setDraft(own ?? "");
    setNaming({ game: namesGame, id: entry.id });
  }

  function commitName(id: number) {
    if (cancelBlur.current) {
      cancelBlur.current = false;
      return;
    }
    if (ownList !== null) {
      const next = setOwnName(names, ownList, id, draft);
      setSaved({ game: namesGame, names: next });
      if (namesGame) saveNames(next, storage, namesGame);
    }
    setNaming(null);
  }

  function cancelName() {
    cancelBlur.current = true;
    setNaming(null);
  }

  const size = columns * ROWS_PER_PAGE;
  const filtered = useMemo(
    () =>
      filterEntries(entries, query, (entry) =>
        ownList !== null ? nameFor(names, ownList, entry.id) : null,
      ),
    [entries, names, ownList, query],
  );
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

  /**
   * The name of an entry: the fallback with a pencil (hover or focus), the own name in italics,
   * or the text box while it is being named. Lists without own names show the plain name.
   */
  function renderName(entry: Entry, own: string | null, fallback: string): ReactNode {
    const canRename = ownList !== null && namesGame != null && entry.name === null;
    if (!canRename) {
      return own === null ? undefined : (
        <span className="min-w-14 truncate pr-0.5 italic" title={own}>
          {own}
        </span>
      );
    }
    if (editing === entry.id) {
      return (
        <input
          type="text"
          autoFocus
          value={draft}
          maxLength={MAX_NAME_CHARS}
          aria-label={t("ownName.label", { name: fallback })}
          placeholder={fallback}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => commitName(entry.id)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commitName(entry.id);
            } else if (e.key === "Escape") {
              e.preventDefault();
              cancelName();
            }
          }}
          className="h-6 min-w-14 flex-1 rounded-sm border border-input/70 bg-field px-1.5 text-xs italic outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
        />
      );
    }
    return (
      <>
        <span
          className={cn("min-w-14 truncate", own !== null && "pr-0.5 italic")}
          title={own ?? fallback}
        >
          {own ?? fallback}
        </span>
        <button
          type="button"
          aria-label={
            own === null
              ? t("ownName.action", { name: fallback })
              : t("ownName.rename", { name: own })
          }
          title={t("ownName.actionTip")}
          onClick={() => startEdit(entry, own)}
          // No width while hidden, so the "#id" after the name stays in line with the other rows.
          className="inline-flex h-5 w-0 shrink-0 items-center justify-center overflow-hidden rounded-sm text-muted-foreground opacity-0 outline-none transition-opacity group-hover:w-5 group-hover:opacity-100 group-focus-within:w-5 group-focus-within:opacity-100 hover:bg-surface-container hover:text-foreground focus-visible:w-5 focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <Pencil aria-hidden="true" className="size-3" />
        </button>
      </>
    );
  }

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
              const fallback = t(`fallback.${kind}`, { id: entry.id });
              const own = ownList !== null ? nameFor(names, ownList, entry.id) : null;
              const name = entryName(entry, own, (id) => t(`fallback.${kind}`, { id }));
              const remove =
                entry.removable && !entry.field.added
                  ? { path: entry.field.path, name }
                  : undefined;
              // The fallback already carries the number; a game or own name gets "#12" after it.
              const id = entry.name !== null || own !== null ? entry.id : undefined;
              return (
                <FieldCell
                  key={entry.id}
                  field={entry.field}
                  name={name}
                  id={id}
                  nameNode={renderName(entry, own, fallback)}
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
