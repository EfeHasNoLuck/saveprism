import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { Actor, Field } from "@/api/commands";
import { cn } from "@/lib/utils";
import { actorPaths, countPending, useEditorContext } from "./context";
import { InlineField } from "./FieldEditor";
import { ROW_HEIGHT, VALUE_COLUMNS } from "./layout";
import {
  ColumnHeader,
  FieldRow,
  RowBar,
  SectionTitle,
  TableNote,
  TablePanel,
} from "./table";
import { useFieldEdit } from "./useFieldEdit";

/** # · Name · Level · HP · MP · EXP */
const ROSTER_COLUMNS =
  "grid grid-cols-[2.5rem_minmax(6rem,1fr)_repeat(4,minmax(0,8.5rem))] items-center gap-x-2 pr-2 pl-3";

function StatCell({ field, label }: { field: Field; label: string }) {
  const edit = useFieldEdit(field);
  return <InlineField edit={edit} label={label} />;
}

/**
 * The Characters section: a dense roster (Level, HP, MP, EXP editable in place) and, under it,
 * the stat bonuses of the selected character as rows.
 */
export function CharactersTable({
  actors,
  info,
  selectedId,
  onSelect,
  changed,
  query,
  onQuery,
}: {
  actors: Actor[];
  info: string;
  selectedId: number | null;
  onSelect: (id: number) => void;
  changed: number;
  query: string;
  onQuery: (query: string) => void;
}) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  const actor = actors.find((a) => a.id === selectedId) ?? actors[0];
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return actors;
    return actors.filter((a) => a.name.toLowerCase().includes(q) || `#${a.id}`.includes(q));
  }, [actors, query]);

  return (
    <TablePanel>
      <SectionTitle
        title={t("sections.characters")}
        info={info}
        total={actors.length}
        changed={changed}
        query={query}
        onQuery={onQuery}
        placeholder={t("search.characters")}
      />
      <div
        className={cn(
          ROSTER_COLUMNS,
          "h-7 shrink-0 border-b border-border bg-chrome text-[11px] font-medium tracking-wider text-muted-foreground uppercase",
        )}
        aria-hidden="true"
      >
        <span>{t("values.id")}</span>
        <span>{t("actor.name")}</span>
        <span>{t("actor.level")}</span>
        <span>{t("actor.hp")}</span>
        <span>{t("actor.mp")}</span>
        <span>{t("actor.exp")}</span>
      </div>
      {actors.length === 0 ? (
        <TableNote>{t("empty.characters")}</TableNote>
      ) : (
        <div className="dense-scroll min-h-[60px] flex-initial overflow-y-auto" role="list" aria-label={t("sections.characters")}>
          {shown.length === 0 ? <TableNote>{t("search.empty")}</TableNote> : null}
          {shown.map((a) => {
            const active = a.id === actor?.id;
            const rowChanged = countPending(ctx.pending, actorPaths(a)) > 0;
            return (
              <div
                key={a.id}
                role="listitem"
                onClick={() => onSelect(a.id)}
                style={{ height: ROW_HEIGHT }}
                className={cn(
                  ROSTER_COLUMNS,
                  "relative border-b border-border text-[13px]",
                  active ? "bg-primary/10" : rowChanged ? "bg-changed-row" : "hover:bg-surface-container",
                )}
              >
                {rowChanged ? (
                  <RowBar state="changed" />
                ) : active ? (
                  <span aria-hidden="true" className="absolute inset-y-0 left-0 w-[3px] bg-primary" />
                ) : null}
                <span
                  className={cn(
                    "font-mono text-xs tabular-nums",
                    rowChanged ? "font-semibold text-changed-foreground" : "text-muted-foreground",
                  )}
                >
                  #{a.id}
                </span>
                <button
                  type="button"
                  aria-current={active || undefined}
                  onClick={() => onSelect(a.id)}
                  className={cn(
                    "min-w-0 truncate rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
                    active ? "font-semibold text-primary" : "font-medium",
                  )}
                  title={a.name}
                >
                  {a.name}
                </button>
                <StatCell field={a.level} label={`${a.name} · ${t("actor.level")}`} />
                <StatCell field={a.hp} label={`${a.name} · ${t("actor.hp")}`} />
                <StatCell field={a.mp} label={`${a.name} · ${t("actor.mp")}`} />
                {a.exp !== null ? (
                  <StatCell field={a.exp} label={`${a.name} · ${t("actor.exp")}`} />
                ) : (
                  <span aria-hidden="true" className="text-muted-foreground">
                    —
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
      {actor !== undefined && actor.bonuses.length > 0 ? (
        <div className="flex shrink-0 flex-col">
          <div className="flex h-8 items-center gap-2 border-y border-border bg-table-head px-3">
            <h3 className="text-[13px] font-semibold">{t("sections.statBonuses")}</h3>
            <span className="truncate text-[13px] text-muted-foreground">{actor.name}</span>
          </div>
          <ColumnHeader
            columns={VALUE_COLUMNS}
            labels={[t("values.name"), t("values.type"), t("values.value"), t("values.change"), null]}
          />
          <div role="list" aria-label={t("sections.statBonuses")}>
            {actor.bonuses.map((b) => {
              const label = t(`stat.${b.key}`, { defaultValue: b.key });
              return (
                <div key={b.field.path} role="listitem" style={{ height: ROW_HEIGHT }}>
                  <FieldRow name={label} field={b.field} columns={VALUE_COLUMNS} />
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </TablePanel>
  );
}
