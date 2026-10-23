import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Actor, Entry, Field, RpgMakerView } from "@/api/commands";
import { InfoTip } from "@/components/ui/info-tip";
import { cn } from "@/lib/utils";
import { CharactersTable } from "./Characters";
import { actorPaths, countPending, useEditorContext } from "./context";
import { EntryTable, type EntryKind } from "./EntryList";
import { InlineField } from "./FieldEditor";
import { SectionNav, type SectionLink } from "./Sidebar";
import { useFieldEdit } from "./useFieldEdit";

type SectionId = "characters" | "items" | "weapons" | "armors" | "variables" | "switches";

const ENTRY_SECTIONS: { id: Exclude<SectionId, "characters">; kind: EntryKind }[] = [
  { id: "items", kind: "item" },
  { id: "weapons", kind: "weapon" },
  { id: "armors", kind: "armor" },
  { id: "variables", kind: "variable" },
  { id: "switches", kind: "switch" },
];

function Gold({ field }: { field: Field }) {
  const { t } = useTranslation("editor");
  const edit = useFieldEdit(field);
  return (
    <div className="flex shrink-0 items-center gap-2">
      <span className="flex items-center gap-0.5 text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
        {t("sections.gold")}
        <InfoTip text={t("info.gold")} />
      </span>
      <InlineField
        edit={edit}
        label={t("sections.gold")}
        showChange
        inputClassName={cn("w-28 bg-surface-high dark:bg-surface-high", !edit.changed && !edit.invalid && "font-medium text-tertiary")}
      />
    </div>
  );
}

/** The party as chips ("#1 Reid Lv 7"); a click opens that character. */
function PartyChips({ actors, onOpen }: { actors: Actor[]; onOpen: (id: number) => void }) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  if (actors.length === 0) return null;
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span className="shrink-0 text-[11px] font-medium tracking-wider text-muted-foreground uppercase">
        {t("sections.party")}
      </span>
      <ul className="no-scrollbar flex min-w-0 items-center gap-1 overflow-x-auto" role="list">
        {actors.map((a) => {
          const level = ctx.fields.get(a.level.path)?.value ?? a.level.value;
          const changed = countPending(ctx.pending, actorPaths(a)) > 0;
          return (
            <li key={a.id} className="shrink-0">
              <button
                type="button"
                onClick={() => onOpen(a.id)}
                title={t("party.open", { name: a.name })}
                className={cn(
                  "flex h-6 items-center gap-1.5 rounded-sm bg-card px-2 text-xs outline-none hover:bg-surface-container focus-visible:ring-2 focus-visible:ring-ring/60",
                  changed && "ring-1 ring-changed",
                )}
              >
                <span className="font-mono text-[11px] text-muted-foreground">#{a.id}</span>
                <span className="max-w-32 truncate font-medium">{a.name}</span>
                <span className="rounded-sm bg-surface-high px-1 font-mono text-[11px] text-primary tabular-nums">
                  {t("actor.lv", { level })}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function countEntries(pending: ReadonlySet<string>, entries: Entry[]): number {
  if (pending.size === 0) return 0;
  return countPending(
    pending,
    entries.map((e) => e.field.path),
  );
}

/**
 * RPG Maker saves: Gold and the party in a strip on top, the sections on the left and one dense
 * table for the selected section.
 */
export function RpgMaker({ view }: { view: RpgMakerView }) {
  const { t } = useTranslation("editor");
  const ctx = useEditorContext();
  const [section, setSection] = useState<SectionId>(
    view.actors.length > 0 ? "characters" : "items",
  );
  const [selectedActor, setSelectedActor] = useState<number | null>(view.actors[0]?.id ?? null);
  const [queries, setQueries] = useState<Partial<Record<SectionId, string>>>({});
  const setQuery = (id: SectionId) => (query: string) =>
    setQueries((prev) => ({ ...prev, [id]: query }));

  const { pending } = ctx;
  const links = useMemo<SectionLink[]>(
    () => [
      {
        id: "characters",
        name: t("sections.characters"),
        count: view.actors.length,
        changed: countPending(pending, view.actors.flatMap(actorPaths)),
      },
      ...ENTRY_SECTIONS.map(({ id }) => ({
        id,
        name: t(`sections.${id}`),
        count: view[id].length,
        changed: countEntries(pending, view[id]),
      })),
    ],
    [pending, t, view],
  );
  const changedOf = (id: SectionId) => links.find((l) => l.id === id)?.changed ?? 0;

  const namesInfo = view.namesFolder === null ? t("info.noNames") : undefined;
  const infos: Record<Exclude<SectionId, "characters">, string | undefined> = {
    items: namesInfo,
    weapons: undefined,
    armors: undefined,
    variables: t("info.variables"),
    switches: t("info.switches"),
  };
  const entrySection = ENTRY_SECTIONS.find((s) => s.id === section);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex h-10 shrink-0 items-center gap-5 rounded-md border border-border bg-chrome px-3">
        <Gold field={view.gold} />
        <PartyChips
          actors={view.actors}
          onOpen={(id) => {
            setSelectedActor(id);
            setSection("characters");
          }}
        />
      </div>
      <div className="flex min-h-0 flex-1 gap-2">
        <SectionNav
          sections={links}
          selected={section}
          onSelect={(id) => setSection(id as SectionId)}
        />
        {entrySection === undefined ? (
          <CharactersTable
            actors={view.actors}
            info={
              view.namesFolder
                ? t("info.namesFrom", { folder: view.namesFolder })
                : t("info.characters")
            }
            selectedId={selectedActor}
            onSelect={setSelectedActor}
            changed={changedOf("characters")}
            query={queries.characters ?? ""}
            onQuery={setQuery("characters")}
          />
        ) : (
          <EntryTable
            key={entrySection.id}
            title={t(`sections.${entrySection.id}`)}
            info={infos[entrySection.id]}
            kind={entrySection.kind}
            entries={view[entrySection.id]}
            changed={changedOf(entrySection.id)}
            query={queries[entrySection.id] ?? ""}
            onQuery={setQuery(entrySection.id)}
          />
        )}
      </div>
    </div>
  );
}
