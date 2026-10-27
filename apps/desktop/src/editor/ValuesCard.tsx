import { ChevronRight, Lock } from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  valueChildren,
  valueSearch,
  type Field,
  type RowNote,
  type ValueRow,
} from "@/api/commands";
import { Button } from "@/components/ui/button";
import { InfoTip } from "@/components/ui/info-tip";
import { cn } from "@/lib/utils";
import { AddValueButton } from "./AddValue";
import { AskAiButton } from "./AskAiButton";
import type { AskAiGroup, AskAiRow } from "./askAi";
import { countPendingUnder, useEditorContext, useFormatNumber } from "./context";
import { RemoveValueButton } from "./RemoveValue";
import { FieldCell } from "./FieldEditor";
import { useGridColumns } from "./grid";
import {
  isEmptyContainerPreview,
  noteText,
  pageCount,
  pageItems,
  readOnlyText,
  ROWS_PER_PAGE,
  searchCover,
} from "./helpers";
import { DenseGrid, EmptyText, Pager, SearchBox, Section } from "./parts";

const CHILD_PAGE = 200;
/** Most search hits for which the groups that hold them open by themselves. */
const AUTO_OPEN_HITS = 300;
/** Wait after the last key before the file is searched, in milliseconds. */
const SEARCH_DELAY = 150;

/** A value or group as the Values card shows it. */
interface ValueNode {
  name: string;
  path: string;
  /**
   * Path of the entry itself in its group; `path` lies below it when the host folded a wrapper
   * (a map entry's row edits the entry's value). The search marks entries, so both are checked.
   */
  entry: string;
  /** The editable value, or null for a group or a value that cannot be edited. */
  field: Field | null;
  children: number;
  /** Short text of a value that cannot be edited. */
  preview: string;
  /** Why a story value is read-only (shown instead of `preview`), else null. */
  note: RowNote | null;
  /** A value can be added to this group. */
  addable: boolean;
  /** The row can be removed from the file. */
  removable: boolean;
  isGroup: boolean;
  /** An empty list or object that takes no values: shown as "empty", like an empty group. */
  empty: boolean;
}

function fromRow(row: ValueRow, entry: string = row.path): ValueNode {
  return {
    name: row.name,
    path: row.path,
    entry,
    field: row.field,
    children: row.children,
    preview: row.preview,
    note: row.note,
    addable: row.addable,
    removable: row.removable,
    isGroup: row.field === null && (row.children > 0 || row.addable),
    empty: row.field === null && !row.addable && isEmptyContainerPreview(row.preview),
  };
}

/** One page of the children of a group, or null when the host refused. */
async function fetchChildren(
  path: string,
  offset: number,
): Promise<{ described: ValueNode[]; total: number } | null> {
  try {
    const page = await valueChildren(path, offset, CHILD_PAGE);
    return { described: page.items.map((child) => fromRow(child, child.entry)), total: page.total };
  } catch {
    return null;
  }
}

/** What the file search found: the hits with every group above them. */
interface Hits {
  covered: ReadonlySet<string>;
  inside: ReadonlySet<string>;
  total: number;
  truncated: boolean;
}

/** The card's search as the groups below the top need it. */
interface SearchState {
  /** The query, trimmed and in lower case; "" when not searching. */
  q: string;
  /** What the host found for `q`, or null while it searches (names and values still match). */
  hits: Hits | null;
}

const NO_HITS: Hits = { covered: new Set(), inside: new Set(), total: 0, truncated: false };

/** Searches the whole file (every depth) for `query`, a moment after the last key. */
function useValueSearch(query: string): SearchState {
  const q = query.trim().toLowerCase();
  const [result, setResult] = useState<{ q: string; hits: Hits } | null>(null);
  useEffect(() => {
    if (q === "") return;
    let cancelled = false;
    const handle = window.setTimeout(() => {
      valueSearch(q).then(
        (found) => {
          if (cancelled) return;
          const { covered, inside } = searchCover(found.paths);
          setResult({ q, hits: { covered, inside, total: found.total, truncated: found.truncated } });
        },
        () => {
          if (!cancelled) setResult({ q, hits: NO_HITS });
        },
      );
    }, SEARCH_DELAY);
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [q]);
  return useMemo(
    () => ({ q, hits: result !== null && result.q === q ? result.hits : null }),
    [q, result],
  );
}

function matches(node: ValueNode, q: string): boolean {
  return (
    node.name.toLowerCase().includes(q) ||
    (node.field !== null && node.field.value.toLowerCase().includes(q))
  );
}

/** True when the search shows `node`: its name or value holds the query, or something at or in it was found. */
function found(node: ValueNode, search: SearchState): boolean {
  if (search.q === "" || matches(node, search.q)) return true;
  const covered = search.hits?.covered;
  return covered !== undefined && (covered.has(node.entry) || covered.has(node.path));
}

/** True when a shown group opens by itself for the search: it or something in it was found, and not too much in all. */
function opensForSearch(node: ValueNode, search: SearchState): boolean {
  const hits = search.hits;
  if (search.q === "" || hits === null || hits.total > AUTO_OPEN_HITS) return false;
  return (
    node.name.toLowerCase().includes(search.q) ||
    hits.inside.has(node.path) ||
    hits.inside.has(node.entry)
  );
}

/** A grid cell for a value: its editor, or its text when it cannot be edited. */
function LeafCell({ node }: { node: ValueNode }) {
  const { t, i18n } = useTranslation("editor");
  const format = useFormatNumber();
  if (node.note !== null) {
    const { text, tip } = noteText(node.note, t, format, i18n.resolvedLanguage ?? "en");
    return (
      <div
        data-field-path={node.path}
        className="flex min-h-7 min-w-0 items-center gap-1.5 rounded-sm pr-2 pl-2 text-xs"
        title={`${node.name} · ${tip}`}
      >
        <span className="min-w-0 truncate">{node.name}</span>
        <span className="ml-auto flex min-w-0 items-center gap-1 text-[0.6875rem] text-muted-foreground">
          <Lock aria-hidden="true" className="size-3 shrink-0" />
          <span className="min-w-0 truncate">{text}</span>
        </span>
      </div>
    );
  }
  if (node.field !== null) {
    const typeName = node.field.typeName;
    const tip = [
      typeName ? t("type.source", { name: typeName }) : null,
      node.field.editable ? null : readOnlyText(node.field.readOnlyReason, t),
    ].filter((part) => part !== null);
    return (
      <FieldCell
        field={node.field}
        name={node.name}
        title={tip.length > 0 ? [node.name, ...tip].join(" · ") : undefined}
        remove={
          node.removable && !node.field.added
            ? { path: node.path, name: node.name }
            : undefined
        }
      />
    );
  }
  return (
    <div
      data-field-path={node.path}
      className={cn(
        "flex min-h-7 min-w-0 items-center gap-1.5 rounded-sm pl-2 text-xs",
        node.removable ? "pr-0.5" : "pr-2",
      )}
    >
      <span className="min-w-0 truncate" title={node.name}>
        {node.name}
      </span>
      {node.empty ? (
        <span className="ml-auto shrink-0 text-[0.6875rem] text-muted-foreground">
          {t("note.empty")}
        </span>
      ) : (
        <span
          className="ml-auto min-w-0 truncate font-mono text-[0.6875rem] text-muted-foreground"
          title={node.preview}
        >
          {node.preview}
        </span>
      )}
      {node.removable ? <RemoveValueButton path={node.path} name={node.name} /> : null}
    </div>
  );
}

/** Values of one level in as many columns as fit (measured here, for nested groups). */
function LeafGrid({ nodes }: { nodes: ValueNode[] }) {
  const { ref, columns } = useGridColumns();
  return (
    <div ref={ref} className="min-w-0">
      {nodes.length > 0 ? (
        <DenseGrid columns={columns} count={nodes.length}>
          {nodes.map((n) => (
            <LeafCell key={`${n.path}|${n.name}`} node={n} />
          ))}
        </DenseGrid>
      ) : null}
    </div>
  );
}

/** True when a found child of `path` is not loaded yet (the search found more than one page holds). */
function missesFound(
  path: string,
  nodes: readonly ValueNode[],
  hits: Hits | null,
): boolean {
  if (hits === null) return false;
  const prefix = `${path}/`;
  const loaded = new Set(nodes.map((node) => node.entry));
  for (const covered of hits.covered) {
    if (
      covered.startsWith(prefix) &&
      !covered.includes("/", prefix.length) &&
      !loaded.has(covered)
    ) {
      return true;
    }
  }
  return false;
}

/**
 * The children of an open group: its values in a grid, then its own groups; "Show more" loads
 * the next 200. While searching, only what the search found shows, unless the group's own name
 * matched (`all`).
 */
function GroupChildren({
  path,
  search,
  all,
}: {
  path: string;
  search: SearchState;
  all: boolean;
}) {
  const { t } = useTranslation("editor");
  const { reveal } = useEditorContext();
  const [nodes, setNodes] = useState<ValueNode[] | null>(null);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    void fetchChildren(path, 0).then((result) => {
      if (cancelled) return;
      setNodes(result?.described ?? []);
      setTotal(result?.total ?? 0);
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [path]);

  // A compare row deep in a long group, or a found value past the first page: load pages until
  // the row (or the group on its way) is in.
  const wanted =
    reveal !== null && reveal.path.startsWith(`${path}/`) ? reveal.path : null;
  const searching = search.q !== "" && !all;
  useEffect(() => {
    if (nodes === null || loading || nodes.length >= total) return;
    const revealing =
      wanted !== null &&
      !nodes.some((node) => wanted === node.path || wanted.startsWith(`${node.path}/`));
    if (!revealing && !(searching && missesFound(path, nodes, search.hits))) return;
    let cancelled = false;
    const handle = window.requestAnimationFrame(() => {
      setLoading(true);
      void fetchChildren(path, nodes.length).then((result) => {
        if (cancelled) return;
        if (result !== null) {
          setNodes((prev) => [...(prev ?? []), ...result.described]);
          setTotal(result.total);
        }
        setLoading(false);
      });
    });
    return () => {
      cancelled = true;
      window.cancelAnimationFrame(handle);
    };
  }, [wanted, path, nodes, total, loading, searching, search.hits]);

  function loadMore() {
    if (nodes === null) return;
    setLoading(true);
    const offset = nodes.length;
    void fetchChildren(path, offset).then((result) => {
      if (result !== null) {
        setNodes((prev) => [...(prev ?? []), ...result.described]);
        setTotal(result.total);
      }
      setLoading(false);
    });
  }

  if (nodes === null) return <EmptyText>{t("loading")}</EmptyText>;
  const leaves = nodes.filter((n) => !n.isGroup && (!searching || found(n, search)));
  const groups = nodes.filter((n) => n.isGroup);
  // While searching, the pages left hold nothing found (unless the search was cut short).
  const more = nodes.length < total && (!searching || search.hits?.truncated !== false);

  return (
    <div className="flex flex-col gap-1">
      <LeafGrid nodes={leaves} />
      {groups.map((g) => (
        <GroupBlock key={`${g.path}|${g.name}`} node={g} search={search} all={all} />
      ))}
      {more ? (
        <div>
          <Button variant="ghost" size="xs" disabled={loading} onClick={loadMore}>
            {t("values.more")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/** The heading line of a group: the open/close button with its name and counts, then `actions`. */
function GroupHeading({
  open,
  onToggle,
  name,
  icon,
  count,
  changed,
  actions,
}: {
  open: boolean;
  onToggle: () => void;
  name: string;
  icon?: ReactNode;
  count: number;
  changed: number;
  actions?: ReactNode;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  return (
    <div className="flex h-7 min-w-0 items-center gap-0.5 rounded-sm pr-0.5 hover:bg-surface-container">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-sm px-1 text-left text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <ChevronRight
          aria-hidden="true"
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-90",
          )}
        />
        {icon}
        <span className="min-w-0 truncate font-semibold" title={name}>
          {name}
        </span>
        <span className="shrink-0 text-[0.6875rem] text-muted-foreground tabular-nums">
          {count === 0 ? t("note.empty") : t("values.items", { count })}
        </span>
        {changed > 0 ? (
          <span className="shrink-0 text-[0.6875rem] font-medium text-changed-foreground tabular-nums">
            {t("count.changed", { count: changed, changed: format(changed) })}
          </span>
        ) : null}
      </button>
      {actions}
    </div>
  );
}

/** Opens a group (once a frame has passed) when `wanted` turns non-null or changes. */
function useOpenWhen(wanted: unknown, setOpen: (open: boolean) => void) {
  useEffect(() => {
    if (wanted === null || wanted === false) return;
    const handle = window.requestAnimationFrame(() => setOpen(true));
    return () => window.cancelAnimationFrame(handle);
  }, [wanted, setOpen]);
}

/**
 * A group as a small heading ("› Player · 12 items · 1 changed") that opens to show its values.
 * While searching, it shows when its name matches or the search found something in it (then it
 * opens by itself), or when an enclosing group's name matched (`all`).
 */
function GroupBlock({
  node,
  search,
  all = false,
  defaultOpen = false,
}: {
  node: ValueNode;
  search: SearchState;
  all?: boolean;
  defaultOpen?: boolean;
}) {
  const { pending, reveal } = useEditorContext();
  const [open, setOpen] = useState(defaultOpen);
  const changed = countPendingUnder(pending, node.path);
  // Open for a value inside the group and for the group itself (a compare row that names a whole
  // container has no row of its own, so its values are shown instead).
  const wanted =
    reveal !== null && (reveal.path === node.path || reveal.path.startsWith(`${node.path}/`))
      ? reveal.nonce
      : null;
  useOpenWhen(wanted, setOpen);
  const auto = !all && opensForSearch(node, search) ? search.q : null;
  useOpenWhen(auto, setOpen);
  if (search.q !== "" && !all && !found(node, search)) return null;
  const named = search.q !== "" && node.name.toLowerCase().includes(search.q);

  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <GroupHeading
        open={open}
        onToggle={() => setOpen((o) => !o)}
        name={node.name}
        count={node.children}
        changed={changed}
        actions={
          <>
            {node.addable ? <AddValueButton parent={node.path} compact /> : null}
            {node.removable ? <RemoveValueButton path={node.path} name={node.name} /> : null}
          </>
        }
      />
      {open ? (
        <div className="ml-2.5 border-l border-border pl-2">
          <GroupChildren path={node.path} search={search} all={all || named} />
        </div>
      ) : null}
    </div>
  );
}

/**
 * What describes the file rather than the game (a GVAS save's header, type tables and save type),
 * as one closed group after the values. Everything in it is read-only.
 */
function DetailsBlock({ nodes, search }: { nodes: ValueNode[]; search: SearchState }) {
  const { t } = useTranslation("editor");
  const { pending, reveal } = useEditorContext();
  const [open, setOpen] = useState(false);
  const shown = search.q === "" ? nodes : nodes.filter((node) => found(node, search));
  const wanted =
    reveal !== null &&
    nodes.some((node) => reveal.path === node.path || reveal.path.startsWith(`${node.path}/`))
      ? reveal.nonce
      : null;
  useOpenWhen(wanted, setOpen);
  const hits = search.hits;
  const auto =
    search.q !== "" && hits !== null && hits.total <= AUTO_OPEN_HITS && shown.length > 0
      ? search.q
      : null;
  useOpenWhen(auto, setOpen);
  if (shown.length === 0) return null;
  const changed = nodes.reduce((sum, node) => sum + countPendingUnder(pending, node.path), 0);

  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <GroupHeading
        open={open}
        onToggle={() => setOpen((o) => !o)}
        name={t("values.details")}
        icon={<Lock aria-hidden="true" className="size-3 shrink-0 text-muted-foreground" />}
        count={nodes.length}
        changed={changed}
        actions={<InfoTip text={t("values.detailsTip")} />}
      />
      {open ? (
        <div className="ml-2.5 flex flex-col gap-1 border-l border-border pl-2">
          <LeafGrid nodes={shown.filter((node) => !node.isGroup)} />
          {shown
            .filter((node) => node.isGroup)
            .map((node) => (
              <GroupBlock key={`${node.path}|${node.name}`} node={node} search={search} />
            ))}
        </div>
      ) : null}
    </div>
  );
}

/**
 * Easy Save 3, GVAS and plain JSON saves: one Values card. The top-level values flow in as many
 * columns as fit, a page at a time; groups follow as headings that open in place, then the file
 * details (`details`) when the file has any. The story view reuses it for the story variables
 * with its own `title` and `info`.
 */
export function ValuesCard({
  rows,
  details,
  title,
  info,
  className,
  addParent,
}: {
  rows: ValueRow[];
  /** What describes the file rather than the game, shown read-only after the values. */
  details?: ValueRow[];
  title?: string;
  info?: string;
  className?: string;
  /** Path a new top-level value goes into, or null/undefined when none can be added. */
  addParent?: string | null;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending, fields, reveal } = useEditorContext();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const { ref, columns } = useGridColumns();
  const search = useValueSearch(query);
  const q = search.q;

  const nodes = useMemo(() => rows.map((row) => fromRow(row)), [rows]);
  const detailNodes = useMemo(() => (details ?? []).map((row) => fromRow(row)), [details]);
  const leaves = useMemo(() => nodes.filter((n) => !n.isGroup), [nodes]);
  const groups = useMemo(() => nodes.filter((n) => n.isGroup), [nodes]);
  const filtered = useMemo(
    () => (q === "" ? leaves : leaves.filter((n) => found(n, search))),
    [leaves, q, search],
  );
  const shownGroups = useMemo(
    () => (q === "" ? groups : groups.filter((g) => found(g, search))),
    [groups, q, search],
  );
  const shownDetails = q === "" ? detailNodes : detailNodes.filter((n) => found(n, search));
  // The values "Ask AI" copies: the filtered leaves, on every page (read-only rows have no value).
  const askRows = useMemo<AskAiRow[]>(
    () =>
      filtered.flatMap((n) => {
        const field = n.field === null ? null : (fields.get(n.path) ?? n.field);
        if (field === null) return [];
        return [{
          name: n.name,
          path: field.path,
          value: field.value,
          kind: field.kind,
          min: field.min,
          max: field.max,
        }];
      }),
    [fields, filtered],
  );
  const size = columns * ROWS_PER_PAGE;
  // A one-sided value from a compare row: drop the card's own search and turn to its page, so
  // the row and every group on the way are on screen. Groups open themselves.
  useEffect(() => {
    if (reveal === null) return;
    const index = leaves.findIndex((node) => node.path === reveal.path);
    const handle = window.requestAnimationFrame(() => {
      setQuery("");
      if (index >= 0) setPage(Math.floor(index / size));
    });
    return () => window.cancelAnimationFrame(handle);
  }, [reveal, leaves, size]);
  const shown = pageItems(filtered, page, size);
  // "Ask AI" also reads the values inside the shown groups, when it is pressed.
  const askGroups = useMemo<AskAiGroup[]>(
    () => shownGroups.map((g) => ({ path: g.path, name: g.name })),
    [shownGroups],
  );
  // A file with one group opens it right away.
  const openFirst = groups.length === 1;
  const hits = search.hits;
  const foundCount = hits === null ? filtered.length : Math.max(hits.total, filtered.length);
  const count =
    q === ""
      ? format(rows.length)
      : t("values.found", {
          count: foundCount,
          shown: `${format(foundCount)}${hits?.truncated === true ? "+" : ""}`,
        });
  const nothing =
    q !== "" &&
    hits !== null &&
    filtered.length === 0 &&
    shownGroups.length === 0 &&
    shownDetails.length === 0;

  return (
    <Section
      title={title ?? t("sections.values")}
      count={count}
      changed={pending.size}
      info={info ?? t("info.values")}
      className={className}
      aside={
        rows.length === 0 && addParent == null ? null : (
          <>
            {rows.length > 0 ? (
              <SearchBox
                value={query}
                onChange={(v) => {
                  setQuery(v);
                  setPage(0);
                }}
                placeholder={t("search.values")}
              />
            ) : null}
            {pageCount(filtered.length, size) > 1 ? (
              <Pager page={page} total={filtered.length} size={size} onPage={setPage} />
            ) : null}
            {addParent != null ? <AddValueButton parent={addParent} /> : null}
            <AskAiButton
              list={title ?? t("sections.values")}
              query={query}
              rows={askRows}
              total={askRows.length}
              groups={askGroups}
            />
          </>
        )
      }
    >
      <div ref={ref} className="flex min-w-0 flex-col gap-1">
        {rows.length === 0 && detailNodes.length === 0 ? (
          <EmptyText>{t("empty.values")}</EmptyText>
        ) : (
          <>
            {shown.length > 0 ? (
              <DenseGrid columns={columns} count={shown.length}>
                {shown.map((n) => (
                  <LeafCell key={`${n.path}|${n.name}`} node={n} />
                ))}
              </DenseGrid>
            ) : nothing ? (
              <EmptyText>{t("search.empty")}</EmptyText>
            ) : null}
            {groups.length > 0 || detailNodes.length > 0 ? (
              <div className={cn("flex flex-col gap-0.5", shown.length > 0 && "border-t border-border pt-1.5")}>
                {groups.map((g) => (
                  <GroupBlock key={`${g.path}|${g.name}`} node={g} search={search} defaultOpen={openFirst} />
                ))}
                {detailNodes.length > 0 ? <DetailsBlock nodes={detailNodes} search={search} /> : null}
              </div>
            ) : null}
          </>
        )}
      </div>
    </Section>
  );
}
