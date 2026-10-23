import { ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { fieldInfo, getChildren, type Field, type NodeRow, type ValueRow } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { countPendingUnder, useEditorContext, useFormatNumber } from "./context";
import { FieldCell } from "./FieldEditor";
import { useGridColumns } from "./grid";
import { pageCount, pageItems, ROWS_PER_PAGE } from "./helpers";
import { DenseGrid, EmptyText, Pager, SearchBox, Section } from "./parts";

const CHILD_PAGE = 200;

/** A value or group as the Values card shows it. */
interface ValueNode {
  name: string;
  path: string;
  /** The editable value, or null for a group or a value that cannot be edited. */
  field: Field | null;
  children: number;
  /** Short text of a value that cannot be edited. */
  preview: string;
  isGroup: boolean;
}

function fromRow(row: ValueRow): ValueNode {
  return {
    name: row.name,
    path: row.path,
    field: row.field,
    children: row.children,
    preview: "",
    isGroup: row.field === null && row.children > 0,
  };
}

async function describeChild(row: NodeRow): Promise<ValueNode> {
  const isGroup = row.kind === "object" || row.kind === "array";
  let field: Field | null = null;
  if (!isGroup) {
    try {
      field = await fieldInfo(row.path);
    } catch {
      field = null; // not editable: shown as read-only text
    }
  }
  return {
    name: row.key,
    path: row.path,
    field,
    children: row.childCount,
    preview: row.preview,
    isGroup,
  };
}

/** One page of described children, or null when the host refused. */
async function fetchChildren(
  path: string,
  offset: number,
): Promise<{ described: ValueNode[]; total: number } | null> {
  try {
    const page = await getChildren(path, offset, CHILD_PAGE);
    const described = await Promise.all(page.items.map(describeChild));
    return { described, total: page.total };
  } catch {
    return null;
  }
}

function matches(node: ValueNode, q: string): boolean {
  return (
    node.name.toLowerCase().includes(q) ||
    (node.field !== null && node.field.value.toLowerCase().includes(q))
  );
}

/** A grid cell for a value: its editor, or its text when it cannot be edited. */
function LeafCell({ node }: { node: ValueNode }) {
  const { t } = useTranslation("editor");
  if (node.field !== null) {
    const typeName = node.field.typeName;
    return (
      <FieldCell
        field={node.field}
        name={node.name}
        title={typeName ? `${node.name} · ${t("type.source", { name: typeName })}` : undefined}
      />
    );
  }
  return (
    <div className="flex h-7 min-w-0 items-center gap-1.5 rounded-sm pr-2 pl-2 text-xs">
      <span className="min-w-0 truncate" title={node.name}>
        {node.name}
      </span>
      <span
        className="ml-auto min-w-0 truncate font-mono text-[11px] text-muted-foreground"
        title={node.preview}
      >
        {node.preview}
      </span>
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
            <LeafCell key={n.path} node={n} />
          ))}
        </DenseGrid>
      ) : null}
    </div>
  );
}

/**
 * The children of an open group: its values in a grid, then its own groups; "Show more" loads
 * the next 200.
 */
function GroupChildren({ path, query }: { path: string; query: string }) {
  const { t } = useTranslation("editor");
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
  const q = query.trim().toLowerCase();
  const leaves = nodes.filter((n) => !n.isGroup && (q === "" || matches(n, q)));
  const groups = nodes.filter((n) => n.isGroup);

  return (
    <div className="flex flex-col gap-1">
      <LeafGrid nodes={leaves} />
      {groups.map((g) => (
        <GroupBlock key={g.path} node={g} query={query} />
      ))}
      {nodes.length < total ? (
        <div>
          <Button variant="ghost" size="xs" disabled={loading} onClick={loadMore}>
            {t("values.more")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/**
 * A group as a small heading ("› Player · 12 items · 1 changed") that opens to show its values.
 * While searching, a closed group shows only when its name matches.
 */
function GroupBlock({
  node,
  query,
  defaultOpen = false,
}: {
  node: ValueNode;
  query: string;
  defaultOpen?: boolean;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending } = useEditorContext();
  const [open, setOpen] = useState(defaultOpen);
  const changed = countPendingUnder(pending, node.path);
  const q = query.trim().toLowerCase();
  if (!open && q !== "" && !node.name.toLowerCase().includes(q)) return null;

  return (
    <div className="flex min-w-0 flex-col gap-0.5">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className="flex h-7 min-w-0 items-center gap-1.5 rounded-sm px-1 text-left text-xs outline-none hover:bg-surface-container focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <ChevronRight
          aria-hidden="true"
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-90",
          )}
        />
        <span className="min-w-0 truncate font-semibold" title={node.name}>
          {node.name}
        </span>
        <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums">
          {t("values.items", { count: node.children })}
        </span>
        {changed > 0 ? (
          <span className="shrink-0 text-[11px] font-medium text-changed-foreground tabular-nums">
            {t("count.changed", { count: changed, changed: format(changed) })}
          </span>
        ) : null}
      </button>
      {open ? (
        <div className="ml-2.5 border-l border-border pl-2">
          <GroupChildren path={node.path} query={query} />
        </div>
      ) : null}
    </div>
  );
}

/**
 * Easy Save 3, GVAS and plain JSON saves: one Values card. The top-level values flow in as many
 * columns as fit, a page at a time; groups follow as headings that open in place.
 */
export function ValuesCard({ rows }: { rows: ValueRow[] }) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { pending } = useEditorContext();
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const { ref, columns } = useGridColumns();

  const nodes = useMemo(() => rows.map(fromRow), [rows]);
  const leaves = useMemo(() => nodes.filter((n) => !n.isGroup), [nodes]);
  const groups = useMemo(() => nodes.filter((n) => n.isGroup), [nodes]);
  const q = query.trim().toLowerCase();
  const filtered = useMemo(
    () => (q === "" ? leaves : leaves.filter((n) => matches(n, q))),
    [leaves, q],
  );
  const size = columns * ROWS_PER_PAGE;
  const shown = pageItems(filtered, page, size);
  // A file with one group and nothing else opens it right away.
  const openFirst = leaves.length === 0 && groups.length === 1;
  const count = q === "" ? format(rows.length) : `${format(filtered.length)} / ${format(leaves.length)}`;

  return (
    <Section
      title={t("sections.values")}
      count={count}
      changed={pending.size}
      info={t("info.values")}
      aside={
        rows.length === 0 ? null : (
          <>
            <SearchBox
              value={query}
              onChange={(v) => {
                setQuery(v);
                setPage(0);
              }}
              placeholder={t("search.values")}
            />
            {pageCount(filtered.length, size) > 1 ? (
              <Pager page={page} total={filtered.length} size={size} onPage={setPage} />
            ) : null}
          </>
        )
      }
    >
      <div ref={ref} className="flex min-w-0 flex-col gap-1">
        {rows.length === 0 ? (
          <EmptyText>{t("empty.values")}</EmptyText>
        ) : (
          <>
            {shown.length > 0 ? (
              <DenseGrid columns={columns} count={shown.length}>
                {shown.map((n) => (
                  <LeafCell key={n.path} node={n} />
                ))}
              </DenseGrid>
            ) : q !== "" && leaves.length > 0 ? (
              <EmptyText>{t("search.empty")}</EmptyText>
            ) : null}
            {groups.length > 0 ? (
              <div className={cn("flex flex-col gap-0.5", shown.length > 0 && "border-t border-border pt-1.5")}>
                {groups.map((g) => (
                  <GroupBlock key={g.path} node={g} query={query} defaultOpen={openFirst} />
                ))}
              </div>
            ) : null}
          </>
        )}
      </div>
    </Section>
  );
}
