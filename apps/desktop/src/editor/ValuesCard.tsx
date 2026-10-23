import { ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { fieldInfo, getChildren, type Field, type NodeRow, type ValueRow } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { countPending, countPendingUnder, useEditorContext } from "./context";
import { VALUE_COLUMNS } from "./layout";
import { SectionNav, type SectionLink } from "./Sidebar";
import {
  ColumnHeader,
  FieldRow,
  PlainRow,
  SectionTitle,
  TableNote,
  TablePanel,
  VirtualRows,
} from "./table";

const CHILD_PAGE = 200;
/** Indent per tree level, in pixels. */
const INDENT = 16;

interface ChildNode {
  name: string;
  path: string;
  field: Field | null;
  children: number;
  preview: string;
  isGroup: boolean;
}

interface Loaded {
  nodes: ChildNode[];
  total: number;
  loading: boolean;
}

type FlatRow =
  | { type: "node"; node: ChildNode; depth: number }
  | { type: "loading"; key: string; depth: number }
  | { type: "more"; path: string; depth: number; loading: boolean };

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

function fromRow(row: ValueRow): ChildNode {
  return {
    name: row.name,
    path: row.path,
    field: row.field,
    children: row.children,
    preview: "",
    isGroup: row.field === null && row.children > 0,
  };
}

interface ValueSection {
  id: string;
  name: string;
  /** The group whose children the section shows, or null for the top-level values. */
  groupPath: string | null;
  /** Top-level values (when `groupPath` is null). */
  rows: ChildNode[];
  count: number;
}

/**
 * Sections of a generic save: the top-level values (if any) and one section per top-level group;
 * one "Values" section when the file has no groups.
 */
function valueSections(rows: ValueRow[], valuesName: string): ValueSection[] {
  const groups = rows.filter((r) => r.field === null && r.children > 0);
  if (groups.length === 0) {
    return [
      { id: "values", name: valuesName, groupPath: null, rows: rows.map(fromRow), count: rows.length },
    ];
  }
  const scalars = rows.filter((r) => !groups.includes(r));
  const sections: ValueSection[] =
    scalars.length > 0
      ? [
          {
            id: "values",
            name: valuesName,
            groupPath: null,
            rows: scalars.map(fromRow),
            count: scalars.length,
          },
        ]
      : [];
  for (const group of groups) {
    sections.push({
      id: `group:${group.path}`,
      name: group.name,
      groupPath: group.path,
      rows: [],
      count: group.children,
    });
  }
  return sections;
}

function matches(node: ChildNode, q: string): boolean {
  return (
    node.name.toLowerCase().includes(q) ||
    (node.field !== null && node.field.value.toLowerCase().includes(q))
  );
}

function Indent({ depth }: { depth: number }) {
  return depth > 0 ? <span className="shrink-0" style={{ width: depth * INDENT }} /> : null;
}

/** The rows of one section as a tree: groups expand in place and load their children on demand. */
function SectionTable({ section, changed }: { section: ValueSection; changed: number }) {
  const { t } = useTranslation("editor");
  const [loaded, setLoaded] = useState<ReadonlyMap<string, Loaded>>(new Map());
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [query, setQuery] = useState("");

  function load(path: string, offset: number) {
    void fetchChildren(path, offset).then((result) => {
      setLoaded((prev) => {
        const next = new Map(prev);
        const before = prev.get(path);
        const nodes = offset === 0 ? [] : (before?.nodes ?? []);
        next.set(path, {
          nodes: result === null ? nodes : [...nodes, ...result.described],
          total: result?.total ?? nodes.length,
          loading: false,
        });
        return next;
      });
    });
  }

  const groupPath = section.groupPath;
  useEffect(() => {
    if (groupPath === null) return;
    let cancelled = false;
    void fetchChildren(groupPath, 0).then((result) => {
      if (cancelled) return;
      setLoaded((prev) =>
        new Map(prev).set(groupPath, {
          nodes: result?.described ?? [],
          total: result?.total ?? 0,
          loading: false,
        }),
      );
    });
    return () => {
      cancelled = true;
    };
  }, [groupPath]);

  function toggle(node: ChildNode) {
    const open = expanded.has(node.path);
    setExpanded((prev) => {
      const next = new Set(prev);
      if (open) next.delete(node.path);
      else next.add(node.path);
      return next;
    });
    if (!open && !loaded.has(node.path)) load(node.path, 0);
  }

  function showMore(path: string) {
    const current = loaded.get(path);
    if (current === undefined || current.loading) return;
    setLoaded((prev) => new Map(prev).set(path, { ...current, loading: true }));
    load(path, current.nodes.length);
  }

  const root = groupPath === null ? section.rows : (loaded.get(groupPath)?.nodes ?? null);
  const rootTotal = groupPath === null ? section.rows.length : (loaded.get(groupPath)?.total ?? 0);
  const rootLoading = groupPath !== null && loaded.get(groupPath)?.loading === true;

  const flat = useMemo(() => {
    const out: FlatRow[] = [];
    const walk = (nodes: ChildNode[], depth: number) => {
      for (const node of nodes) {
        out.push({ type: "node", node, depth });
        if (!node.isGroup || !expanded.has(node.path)) continue;
        const children = loaded.get(node.path);
        if (children === undefined) {
          out.push({ type: "loading", key: `${node.path}#loading`, depth: depth + 1 });
          continue;
        }
        walk(children.nodes, depth + 1);
        if (children.nodes.length < children.total) {
          out.push({ type: "more", path: node.path, depth: depth + 1, loading: children.loading });
        }
      }
    };
    if (root !== null) walk(root, 0);
    if (root !== null && groupPath !== null && root.length < rootTotal) {
      out.push({ type: "more", path: groupPath, depth: 0, loading: rootLoading });
    }
    const q = query.trim().toLowerCase();
    if (q === "") return out;
    return out.filter((row) => row.type === "node" && matches(row.node, q));
  }, [expanded, groupPath, loaded, query, root, rootLoading, rootTotal]);

  return (
    <TablePanel>
      <SectionTitle
        title={section.name}
        info={t("info.values")}
        total={section.count}
        changed={changed}
        query={query}
        onQuery={setQuery}
        placeholder={t("search.values")}
      />
      <ColumnHeader
        columns={VALUE_COLUMNS}
        labels={[t("values.name"), t("values.type"), t("values.value"), t("values.change"), null]}
      />
      {root === null ? (
        <TableNote>{t("loading")}</TableNote>
      ) : root.length === 0 ? (
        <TableNote>{t("empty.values")}</TableNote>
      ) : flat.length === 0 ? (
        <TableNote>{t("search.empty")}</TableNote>
      ) : (
        <VirtualRows
          label={section.name}
          items={flat}
          itemKey={(row) =>
            row.type === "node" ? row.node.path : row.type === "loading" ? row.key : `${row.path}#more`
          }
          render={(row) => {
            if (row.type === "loading") {
              return (
                <PlainRow columns={VALUE_COLUMNS}>
                  <span className="flex min-w-0 items-center text-muted-foreground">
                    <Indent depth={row.depth} />
                    <span className="w-5 shrink-0" />
                    {t("loading")}
                  </span>
                </PlainRow>
              );
            }
            if (row.type === "more") {
              return (
                <PlainRow columns={VALUE_COLUMNS}>
                  <span className="flex min-w-0 items-center">
                    <Indent depth={row.depth} />
                    <span className="w-5 shrink-0" />
                    <Button
                      variant="ghost"
                      size="xs"
                      disabled={row.loading}
                      onClick={() => showMore(row.path)}
                    >
                      {t("values.more")}
                    </Button>
                  </span>
                </PlainRow>
              );
            }
            const { node, depth } = row;
            if (node.field !== null) {
              return (
                <FieldRow
                  name={node.name}
                  field={node.field}
                  columns={VALUE_COLUMNS}
                  lead={
                    <>
                      <Indent depth={depth} />
                      <span className="w-5 shrink-0" />
                    </>
                  }
                />
              );
            }
            const open = expanded.has(node.path);
            return (
              <PlainRow columns={VALUE_COLUMNS}>
                <span className="flex min-w-0 items-center gap-1">
                  <Indent depth={depth} />
                  {node.isGroup ? (
                    <button
                      type="button"
                      aria-expanded={open}
                      aria-label={open ? t("values.collapse") : t("values.expand")}
                      onClick={() => toggle(node)}
                      className="inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground outline-none hover:bg-surface-high hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
                    >
                      <ChevronRight className={cn("size-3.5 transition-transform", open && "rotate-90")} />
                    </button>
                  ) : (
                    <span className="w-5 shrink-0" />
                  )}
                  <span className="truncate" title={node.name}>
                    {node.name}
                  </span>
                  {node.isGroup ? (
                    <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                      {t("values.items", { count: node.children })}
                    </span>
                  ) : null}
                </span>
                <span />
                <span className="col-span-3 truncate font-mono text-xs text-muted-foreground" title={node.preview}>
                  {node.isGroup ? null : node.preview}
                </span>
              </PlainRow>
            );
          }}
        />
      )}
    </TablePanel>
  );
}

/** Easy Save 3, GVAS and plain JSON saves: sections on the left, one dense table on the right. */
export function ValuesView({ rows }: { rows: ValueRow[] }) {
  const { t } = useTranslation("editor");
  const { pending } = useEditorContext();
  const sections = useMemo(() => valueSections(rows, t("sections.values")), [rows, t]);
  const [selected, setSelected] = useState(sections[0]?.id ?? "values");
  const section = sections.find((s) => s.id === selected) ?? sections[0];

  const links = useMemo<SectionLink[]>(
    () =>
      sections.map((s) => ({
        id: s.id,
        name: s.name,
        count: s.count,
        changed:
          s.groupPath === null
            ? countPending(
                pending,
                s.rows.map((r) => r.path),
              )
            : countPendingUnder(pending, s.groupPath),
      })),
    [pending, sections],
  );

  return (
    <div className="flex min-h-0 flex-1 gap-2">
      <SectionNav sections={links} selected={section?.id ?? ""} onSelect={setSelected} />
      {section === undefined ? (
        <TablePanel>
          <SectionTitle title={t("sections.values")} total={0} changed={0} />
          <TableNote>{t("empty.values")}</TableNote>
        </TablePanel>
      ) : (
        <SectionTable
          key={section.id}
          section={section}
          changed={links.find((l) => l.id === section.id)?.changed ?? 0}
        />
      )}
    </div>
  );
}
