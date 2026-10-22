import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Actor, Field } from "@/api/commands";
import { cn } from "@/lib/utils";
import { useEditorContext } from "./context";
import { FieldEditor } from "./FieldEditor";
import { Section } from "./parts";

function Labeled({ label, field }: { label: string; field: Field }) {
  return (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-[13px] text-muted-foreground">{label}</span>
      <FieldEditor field={field} label={label} />
    </div>
  );
}

export function Characters({
  actors,
  namesFolder,
  className,
}: {
  actors: Actor[];
  namesFolder: string | null;
  className?: string;
}) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  const [selectedId, setSelectedId] = useState<number | null>(actors[0]?.id ?? null);
  const actor = actors.find((a) => a.id === selectedId) ?? actors[0];

  const info = namesFolder
    ? t("info.namesFrom", { folder: namesFolder })
    : t("info.characters");

  return (
    <Section title={t("sections.characters")} info={info} className={className}>
      {actor === undefined ? null : (
        <div className="flex flex-col gap-4 sm:flex-row">
          <ul className="flex shrink-0 flex-col gap-0.5 sm:w-48" role="list">
            {actors.map((a) => {
              const level = ctx.fields.get(a.level.path)?.value ?? a.level.value;
              const active = a.id === actor.id;
              return (
                <li key={a.id}>
                  <button
                    type="button"
                    aria-current={active || undefined}
                    onClick={() => setSelectedId(a.id)}
                    className={cn(
                      "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50",
                      active && "bg-primary/10",
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
          <div className="flex min-w-0 flex-1 flex-col gap-4">
            <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
              <Labeled label={t("actor.level")} field={actor.level} />
              <Labeled label={t("actor.hp")} field={actor.hp} />
              <Labeled label={t("actor.mp")} field={actor.mp} />
              {actor.exp !== null ? <Labeled label={t("actor.exp")} field={actor.exp} /> : null}
            </div>
            <div className="flex flex-col gap-2">
              <h3 className="text-[13px] font-semibold">{t("sections.statBonuses")}</h3>
              <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">
                {actor.bonuses.map((b) => (
                  <Labeled key={b.field.path} label={b.label} field={b.field} />
                ))}
              </div>
            </div>
          </div>
        </div>
      )}
    </Section>
  );
}
