import { ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  fieldInfo,
  getChildren,
  type Field,
  type NodeRow,
  type ValueRow,
} from "@/api/commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { isChanged, useEditorContext } from "./context";
import { FieldEditor } from "./FieldEditor";
import { EmptyText, SearchBox, Section } from "./parts";

const CHILD_PAGE = 200;
/** Name · Type · Value. The type column fits "Decimal number" in English and Turkish. */
const COLUMNS = "grid grid-cols-[minmax(0,1fr)_8.5rem_minmax(0,15rem)] items-center gap-3";

/** The kind of a value in words; the file's own type name (Int32) is in the tooltip. */
function TypeTag({ field }: { field: Field }) {
  const { t } = useTranslation("editor");
  const label = t(`type.${field.kind}`);
  return (
    <span
      className="max-w-full truncate rounded bg-muted px-1.5 py-0.5 text-xs leading-4 text-muted-foreground"
      title={field.typeName ? t("type.source", { name: field.typeName }) : label}
    >
      {label}
    </span>
  );
}

interface NodeProps {
  name: string;
  path: string;
  field: Field | null;
  childCount: number;
  depth: number;
}

/** One row: a value with its editor (tinted when changed), or a group that expands to its children. */
function ValueNode({ name, path, field, childCount, depth }: NodeProps) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  const [open, setOpen] = useState(false);
  const indent = { paddingLeft: `${depth * 16 + 8}px` };
  const changed = field !== null && isChanged(ctx.fields, field);

  return (
    <>
      <div
        className={cn(COLUMNS, "min-h-11 border-b py-1.5 pr-2", changed && "bg-changed/10")}
        style={indent}
      >
        <div className="flex min-w-0 items-center gap-1">
          {field === null ? (
            <button
              type="button"
              aria-expanded={open}
              aria-label={open ? t("values.collapse") : t("values.expand")}
              onClick={() => setOpen((o) => !o)}
              className="inline-flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
            </button>
          ) : (
            <span className="size-5 shrink-0" />
          )}
          <span className="truncate text-[13px]" title={name}>
            {name}
          </span>
          {field === null ? (
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
              {t("values.items", { count: childCount })}
            </span>
          ) : null}
        </div>
        <div className="flex min-w-0 items-center">
          {field !== null ? <TypeTag field={field} /> : null}
        </div>
        <div className="min-w-0">
          {field !== null ? <FieldEditor field={field} label={name} floatingHint /> : null}
        </div>
      </div>
      {field === null && open ? <GroupChildren path={path} depth={depth + 1} /> : null}
    </>
  );
}

interface ChildNode {
  name: string;
  path: string;
  field: Field | null;
  children: number;
  preview: string;
  isGroup: boolean;
}

async function describeChild(row: NodeRow): Promise<ChildNode> {
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
): Promise<{ described: ChildNode[]; total: number } | null> {
  try {
    const page = await getChildren(path, offset, CHILD_PAGE);
    const described = await Promise.all(page.items.map(describeChild));
    return { described, total: page.total };
  } catch {
    return null;
  }
}

function GroupChildren({ path, depth }: { path: string; depth: number }) {
  const { t } = useTranslation("editor");
  const [nodes, setNodes] = useState<ChildNode[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);

  function apply(offset: number, result: { described: ChildNode[]; total: number } | null) {
    if (result !== null) {
      setNodes((prev) => (offset === 0 ? result.described : [...prev, ...result.described]));
      setTotal(result.total);
    } else if (offset === 0) {
      setTotal(0);
    }
    setLoading(false);
  }

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
    setLoading(true);
    const offset = nodes.length;
    void fetchChildren(path, offset).then((result) => apply(offset, result));
  }

  return (
    <>
      {nodes.map((n) =>
        n.field === null && !n.isGroup ? (
          <div
            key={n.path}
            className="flex min-h-11 items-center gap-3 border-b py-1.5 pr-2 text-[13px]"
            style={{ paddingLeft: `${depth * 16 + 8 + 24}px` }}
          >
            <span className="truncate">{n.name}</span>
            <span className="truncate text-muted-foreground">{n.preview}</span>
          </div>
        ) : (
          <ValueNode
            key={n.path}
            name={n.name}
            path={n.path}
            field={n.field}
            childCount={n.children}
            depth={depth}
          />
        ),
      )}
      {nodes.length < total ? (
        <div style={{ paddingLeft: `${depth * 16 + 8}px` }} className="py-1.5">
          <Button
            variant="ghost"
            size="sm"
            disabled={loading}
            onClick={loadMore}
          >
            {t("values.more")}
          </Button>
        </div>
      ) : null}
    </>
  );
}

export function ValuesCard({ rows }: { rows: ValueRow[] }) {
  const { t } = useTranslation("editor");
  const [query, setQuery] = useState("");
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return rows;
    return rows.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        (r.field !== null && r.field.value.toLowerCase().includes(q)),
    );
  }, [rows, query]);

  return (
    <Section title={t("sections.values")} info={t("info.values")}>
      {rows.length === 0 ? (
        <EmptyText>{t("empty.values")}</EmptyText>
      ) : (
        <>
          <SearchBox value={query} onChange={setQuery} placeholder={t("search.values")} />
          <div>
            <div
              className={cn(COLUMNS, "border-b py-1.5 pr-2 pl-8 text-xs text-muted-foreground")}
            >
              <span>{t("values.name")}</span>
              <span>{t("values.type")}</span>
              <span>{t("values.value")}</span>
            </div>
            {filtered.length === 0 ? (
              <EmptyText>{t("search.empty")}</EmptyText>
            ) : (
              filtered.map((r) => (
                <ValueNode
                  key={r.path}
                  name={r.name}
                  path={r.path}
                  field={r.field}
                  childCount={r.children}
                  depth={0}
                />
              ))
            )}
          </div>
        </>
      )}
    </Section>
  );
}
