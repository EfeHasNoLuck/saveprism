import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Actor, Field } from "@/api/commands";
import { cn } from "@/lib/utils";
import { isChanged, useEditorContext } from "./context";
import { FieldEditor } from "./FieldEditor";
import { ChangeNote, EmptyText, Section } from "./parts";

/**
 * A stat with its label above; the pair is tinted when the value is changed, and "old → new" sits
 * at the end of the label line so the stat keeps its height.
 */
function Labeled({ label, field }: { label: string; field: Field }) {
  const ctx = useEditorContext();
  return (
    <div
      className={cn(
        "-m-1.5 flex min-w-0 flex-col gap-1 rounded-md p-1.5",
        isChanged(ctx.fields, field) && "bg-changed/10",
      )}
    >
      <div className="flex min-w-0 items-baseline justify-between gap-2">
        <span className="truncate text-[13px] text-muted-foreground">{label}</span>
        <ChangeNote field={field} className="max-w-[65%] shrink-0" />
      </div>
      <FieldEditor field={field} label={label} showChange={false} />
    </div>
  );
}

/**
 * The party: `gold` and the character list on the left, the picked character on the right, so
 * both columns end at the same height.
 */
export function Characters({
  actors,
  namesFolder,
  gold,
}: {
  actors: Actor[];
  namesFolder: string | null;
  gold: ReactNode;
}) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  const [selectedId, setSelectedId] = useState<number | null>(actors[0]?.id ?? null);
  const actor = actors.find((a) => a.id === selectedId) ?? actors[0];

  const info = namesFolder
    ? t("info.namesFrom", { folder: namesFolder })
    : t("info.characters");

  const list = (
    <Section title={t("sections.characters")} info={info} className="flex-1">
      {actors.length === 0 ? (
        <EmptyText>{t("empty.characters")}</EmptyText>
      ) : (
        <ul className="flex flex-col gap-0.5" role="list">
          {actors.map((a) => {
            const level = ctx.fields.get(a.level.path)?.value ?? a.level.value;
            const active = a.id === actor?.id;
            return (
              <li key={a.id}>
                <button
                  type="button"
                  aria-current={active || undefined}
                  onClick={() => setSelectedId(a.id)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50",
                    active && "bg-primary/10 hover:bg-primary/15",
                  )}
                >
                  <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-semibold">
                    {Array.from(a.name)[0]?.toUpperCase() ?? "?"}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] font-medium">{a.name}</span>
                    <span className="block text-xs text-muted-foreground tabular-nums">
                      {t("actor.lv", { level })}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Section>
  );

  if (actor === undefined) {
    return (
      <div className="grid gap-4 lg:grid-cols-2">
        {gold}
        {list}
      </div>
    );
  }

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,3fr)]">
      <div className="flex min-w-0 flex-col gap-4">
        {gold}
        {list}
      </div>
      <Section overline={t("sections.character")} title={actor.name}>
        <div className="flex min-w-0 flex-col gap-5">
          <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
            <Labeled label={t("actor.level")} field={actor.level} />
            <Labeled label={t("actor.hp")} field={actor.hp} />
            <Labeled label={t("actor.mp")} field={actor.mp} />
            {actor.exp !== null ? <Labeled label={t("actor.exp")} field={actor.exp} /> : null}
          </div>
          {actor.bonuses.length > 0 ? (
            <div className="flex flex-col gap-2">
              <h3 className="text-[13px] font-semibold">{t("sections.statBonuses")}</h3>
              <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
                {actor.bonuses.map((b) => (
                  <Labeled
                    key={b.field.path}
                    label={t(`stat.${b.key}`, { defaultValue: b.key })}
                    field={b.field}
                  />
                ))}
              </div>
            </div>
          ) : null}
        </div>
      </Section>
    </div>
  );
}
