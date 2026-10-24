import { Check, ChevronDown } from "lucide-react";
import { Select } from "radix-ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Actor } from "@/api/commands";
import { actorPaths, countPending, useEditorContext, useFormatNumber } from "./context";
import { FieldCell } from "./FieldEditor";
import { EmptyText, Section } from "./parts";

/** The character picker: "#1 Harold · Lv 7", with the number of changed values per character. */
function ActorSelect({
  actors,
  value,
  onChange,
}: {
  actors: Actor[];
  value: number;
  onChange: (id: number) => void;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const ctx = useEditorContext();
  return (
    <Select.Root value={String(value)} onValueChange={(v) => onChange(Number(v))}>
      <Select.Trigger
        aria-label={t("sections.character")}
        className="inline-flex h-7 max-w-64 min-w-0 items-center gap-1.5 rounded-sm border border-input/70 bg-field px-2 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
      >
        <span className="min-w-0 truncate">
          <Select.Value />
        </span>
        <Select.Icon className="shrink-0 text-muted-foreground">
          <ChevronDown className="size-3.5" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content
          position="popper"
          sideOffset={4}
          collisionPadding={8}
          className="z-50 max-h-(--radix-select-content-available-height) min-w-(--radix-select-trigger-width) overflow-hidden rounded-md border border-border bg-popover text-popover-foreground shadow-lg shadow-black/25"
        >
          <Select.Viewport className="p-1">
            {actors.map((a) => {
              const level = ctx.fields.get(a.level.path)?.value ?? a.level.value;
              const changed = countPending(ctx.pending, actorPaths(a));
              return (
                <Select.Item
                  key={a.id}
                  value={String(a.id)}
                  className="relative flex h-7 cursor-default items-center gap-3 rounded-sm pr-2 pl-6 text-xs outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground"
                >
                  <Select.ItemIndicator className="absolute left-1.5 inline-flex">
                    <Check className="size-3.5" />
                  </Select.ItemIndicator>
                  <Select.ItemText>
                    #{a.id} {a.name} · {t("actor.lv", { level })}
                  </Select.ItemText>
                  {changed > 0 ? (
                    <span className="ml-auto text-[0.6875rem] font-medium text-changed-foreground">
                      {t("count.changed", { count: changed, changed: format(changed) })}
                    </span>
                  ) : null}
                </Select.Item>
              );
            })}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}

/**
 * The Characters card: a picker for the character, and under it the picked character's Level, HP,
 * MP, EXP and stat bonuses in a compact grid (label, value, undo).
 */
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
  const format = useFormatNumber();
  const ctx = useEditorContext();
  const [selectedId, setSelectedId] = useState<number | null>(actors[0]?.id ?? null);
  const actor = actors.find((a) => a.id === selectedId) ?? actors[0];
  const changed = useMemo(
    () => countPending(ctx.pending, actors.flatMap(actorPaths)),
    [actors, ctx.pending],
  );

  const info = namesFolder
    ? `${t("info.characters")} ${t("info.namesFrom", { folder: namesFolder })}`
    : t("info.characters");

  const stat = (label: string, field: Actor["level"]) => (
    <FieldCell key={field.path} field={field} name={label} fixedLabel />
  );

  return (
    <Section
      title={t("sections.characters")}
      count={format(actors.length)}
      changed={changed}
      info={info}
      className={className}
      aside={
        actor === undefined ? null : (
          <ActorSelect actors={actors} value={actor.id} onChange={setSelectedId} />
        )
      }
    >
      {actor === undefined ? (
        <EmptyText>{t("empty.characters")}</EmptyText>
      ) : (
        <div className="grid grid-cols-1 gap-x-3 gap-y-0.5 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {stat(t("actor.level"), actor.level)}
          {stat(t("actor.hp"), actor.hp)}
          {stat(t("actor.mp"), actor.mp)}
          {actor.exp !== null ? stat(t("actor.exp"), actor.exp) : null}
          {actor.bonuses.map((b) => stat(t(`stat.${b.key}`, { defaultValue: b.key }), b.field))}
        </div>
      )}
    </Section>
  );
}
