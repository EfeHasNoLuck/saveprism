import { useVirtualizer } from "@tanstack/react-virtual";
import { Filter } from "lucide-react";
import { memo, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Field } from "@/api/commands";
import { InfoTip } from "@/components/ui/info-tip";
import { cn } from "@/lib/utils";
import { useFormatNumber } from "./context";
import { ChangeCell, UndoButton, ValueInput } from "./FieldEditor";
import { useFieldEdit } from "./useFieldEdit";
import { ROW, ROW_HEIGHT, rowTone, type RowState } from "./layout";

/** The white panel that holds a section: title row, column header and rows. */
export function TablePanel({ children }: { children: ReactNode }) {
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-md border border-border bg-card text-card-foreground">
      {children}
    </section>
  );
}

/** Section name, "500 total · 2 changed" and the filter box. */
export function SectionTitle({
  title,
  total,
  changed,
  info,
  query,
  onQuery,
  placeholder,
}: {
  title: string;
  total: number;
  changed: number;
  info?: string;
  query?: string;
  onQuery?: (query: string) => void;
  placeholder?: string;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  return (
    <div className="flex h-10 shrink-0 items-center gap-2 border-b border-border bg-table-head px-3">
      <h2 className="truncate text-base leading-6 font-semibold">{title}</h2>
      <span className="shrink-0 rounded-sm bg-surface-highest px-1.5 py-0.5 font-mono text-[11px] leading-4 text-muted-foreground tabular-nums">
        {t("table.total", { count: total, total: format(total) })}
        {changed > 0 ? (
          <>
            {" · "}
            <span className="font-medium text-changed-foreground">
              {t("table.changed", { count: changed, changed: format(changed) })}
            </span>
          </>
        ) : null}
      </span>
      {info ? <InfoTip text={info} /> : null}
      {onQuery !== undefined && placeholder !== undefined ? (
        <div className="relative ml-auto w-60 min-w-36 shrink">
          <Filter className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <input
            type="search"
            value={query ?? ""}
            aria-label={placeholder}
            placeholder={placeholder}
            onChange={(e) => onQuery(e.target.value)}
            className="h-7 w-full rounded-sm border border-input/70 bg-field pr-2 pl-7 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
          />
        </div>
      ) : null}
    </div>
  );
}

/** The column names; it stays put while the rows scroll. */
export function ColumnHeader({ columns, labels }: { columns: string; labels: (string | null)[] }) {
  return (
    <div
      className={cn(
        "dense-scroll grid h-7 shrink-0 items-center gap-x-3 overflow-hidden border-b border-border bg-chrome pr-2 pl-3 text-[11px] font-medium tracking-wider text-muted-foreground uppercase",
        columns,
      )}
      aria-hidden="true"
    >
      {labels.map((label, index) => (
        <span key={index} className="truncate">
          {label}
        </span>
      ))}
    </div>
  );
}

/** A quiet line in an empty table. */
export function TableNote({ children }: { children: ReactNode }) {
  return <p className="px-3 py-3 text-[13px] text-muted-foreground">{children}</p>;
}

/**
 * Rows drawn only while they are in view, so a section with 10,000 values scrolls as smoothly
 * as one with ten.
 */
export function VirtualRows<T>({
  items,
  itemKey,
  render,
  label,
}: {
  items: readonly T[];
  itemKey: (item: T) => string;
  render: (item: T) => ReactNode;
  /** Accessible name of the list. */
  label: string;
}) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- the virtualizer keeps its own state
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 12,
    getItemKey: (index) => {
      const item = items[index];
      return item === undefined ? index : itemKey(item);
    },
  });
  return (
    <div ref={scrollRef} role="list" aria-label={label} className="dense-scroll min-h-0 flex-1 overflow-y-auto">
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((row) => {
          const item = items[row.index];
          if (item === undefined) return null;
          return (
            <div
              key={row.key}
              role="listitem"
              className="absolute top-0 left-0 w-full"
              style={{ height: row.size, transform: `translateY(${row.start}px)` }}
            >
              {render(item)}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The type of a value as a small tag: the file's own name (Int32) or the kind (int, bool). */
export function TypeTag({ field, state }: { field: Field; state: RowState }) {
  const { t } = useTranslation("editor");
  const label = field.typeName ?? t(`tag.${field.kind}`);
  return (
    <span
      className={cn(
        "max-w-full justify-self-start truncate rounded-sm px-1 font-mono text-[11px] leading-4 uppercase",
        state === "changed"
          ? "bg-changed-tag text-changed-tag-foreground"
          : state === "error"
            ? "bg-error-container text-error-container-foreground"
            : "bg-surface-highest text-muted-foreground",
      )}
      title={field.typeName ? t("type.source", { name: field.typeName }) : t(`type.${field.kind}`)}
    >
      {label}
    </span>
  );
}

/** The violet (changed) or red (cannot be saved) bar on the left edge of a row. */
export function RowBar({ state }: { state: RowState }) {
  if (state === "idle") return null;
  return (
    <span
      aria-hidden="true"
      className={cn(
        "absolute inset-y-0 left-0 w-[3px]",
        state === "changed" ? "bg-changed" : "bg-destructive",
      )}
    />
  );
}

/**
 * One editable value: #id, name, type tag, value control, Change column and undo. A changed row
 * is tinted violet with a violet bar; a row whose typed value cannot be saved is tinted red.
 */
function FieldRowView({
  id,
  name,
  field,
  columns,
  lead,
}: {
  /** Shown as "#12"; leave out for rows without an id (the table has no id column then). */
  id?: number;
  name: string;
  field: Field;
  columns: string;
  /** Indent or icon before the name. */
  lead?: ReactNode;
}) {
  const edit = useFieldEdit(field);
  const state: RowState = edit.invalid ? "error" : edit.changed ? "changed" : "idle";
  return (
    <div className={cn(ROW, columns, rowTone(state))}>
      <RowBar state={state} />
      {id !== undefined ? (
        <span
          className={cn(
            "font-mono text-xs tabular-nums",
            state === "idle" && "text-muted-foreground",
            state === "changed" && "font-semibold text-changed-foreground",
            state === "error" && "font-semibold text-destructive",
          )}
        >
          #{id}
        </span>
      ) : null}
      <span className="flex min-w-0 items-center gap-1">
        {lead}
        <span className="truncate" title={name}>
          {name}
        </span>
      </span>
      <TypeTag field={edit.field} state={state} />
      <ValueInput edit={edit} label={name} />
      <span className="flex min-w-0 items-center">
        <ChangeCell edit={edit} />
      </span>
      <UndoButton edit={edit} />
    </div>
  );
}

/** Rows re-render only when their own props change, not on every scroll step. */
export const FieldRow = memo(FieldRowView);

/** A row with the table's grid but custom cells (groups, read-only values, "Show more"). */
export function PlainRow({ columns, children }: { columns: string; children: ReactNode }) {
  return <div className={cn(ROW, columns, "hover:bg-surface-container")}>{children}</div>;
}
