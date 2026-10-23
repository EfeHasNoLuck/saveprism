import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { Field, RpgMakerView } from "@/api/commands";
import { cn } from "@/lib/utils";
import { Characters } from "./Characters";
import { EntryList, type EntryListProps } from "./EntryList";
import { useElementWidth } from "./grid";
import { allocateColumns, cardWidth } from "./helpers";
import { StateBar, UndoButton, ValueInput } from "./FieldEditor";
import { Section } from "./parts";
import { fieldState, stateTone, useFieldEdit } from "./useFieldEdit";

/**
 * Gold: the value with − and + and its undo; under it "old → new" when changed, the problem when
 * the value cannot be saved, else the type and limits.
 */
function Gold({ field }: { field: Field }) {
  const { t } = useTranslation("editor");
  const edit = useFieldEdit(field);
  const state = fieldState(edit);
  const note = edit.invalid && edit.message !== null ? edit.message : (edit.change ?? edit.typeHint);
  return (
    <Section title={t("sections.gold")} info={t("info.gold")}>
      <div className="flex flex-col gap-1">
        <div
          className={cn(
            "relative flex h-7 min-w-0 items-center gap-1 rounded-sm pr-0.5 pl-2",
            stateTone(state),
          )}
        >
          <StateBar state={state} />
          <ValueInput
            edit={edit}
            label={t("sections.gold")}
            steppers
            className="min-w-0 flex-1"
            inputClassName={cn(state === "idle" && "font-medium text-tertiary")}
          />
          <UndoButton edit={edit} />
        </div>
        <p
          role={edit.invalid ? "alert" : undefined}
          className={cn(
            "truncate pl-2 text-xs",
            state === "error" && "font-medium text-destructive",
            state === "changed" && "font-mono text-changed-foreground tabular-nums",
            state === "idle" && "text-muted-foreground",
          )}
          title={edit.typeHint}
        >
          {note}
        </p>
      </div>
    </Section>
  );
}

/**
 * Flex sizing of a list card with `columns` entry columns: it starts at the width those columns
 * need and shares the rest of the row by its column count. An empty card stays narrow.
 */
function cardFlex(columns: number, onlyEmpty: boolean): CSSProperties {
  if (columns === 0) return { flex: `${onlyEmpty ? 1 : 0} 1 ${cardWidth(0)}px` };
  return { flex: `${columns} 1 ${cardWidth(columns)}px` };
}

/**
 * A row of list cards. The entry columns that fit across the row go to the cards by their
 * length, so a long list gets more columns, an empty one stays narrow, and the cards end at
 * similar heights. The cards wrap under each other when the window is narrow.
 */
function ListRow({ lists }: { lists: Omit<EntryListProps, "style" | "className">[] }) {
  const { ref, width } = useElementWidth();
  const counts = lists.map((l) => l.entries.length);
  const columns = allocateColumns(width, counts);
  const onlyEmpty = counts.every((n) => n === 0);
  return (
    <div ref={ref} className="flex flex-wrap items-start gap-2.5">
      {lists.map((list, i) => (
        <EntryList key={list.kind} {...list} style={cardFlex(columns[i] ?? 0, onlyEmpty)} />
      ))}
    </div>
  );
}

/**
 * RPG Maker saves as cards in three rows: Gold and Characters; Items, Weapons and Armors;
 * Variables and Switches.
 */
export function RpgMaker({ view }: { view: RpgMakerView }) {
  const { t } = useTranslation("editor");
  const namesInfo = view.namesFolder === null ? t("info.noNames") : undefined;
  return (
    <div className="flex flex-col gap-2.5">
      <div className="grid gap-2.5 md:grid-cols-[14rem_minmax(0,1fr)]">
        <Gold field={view.gold} />
        <Characters actors={view.actors} namesFolder={view.namesFolder} />
      </div>
      <ListRow
        lists={[
          { title: t("sections.items"), kind: "item", entries: view.items, info: namesInfo },
          { title: t("sections.weapons"), kind: "weapon", entries: view.weapons },
          { title: t("sections.armors"), kind: "armor", entries: view.armors },
        ]}
      />
      <ListRow
        lists={[
          {
            title: t("sections.variables"),
            info: t("info.variables"),
            kind: "variable",
            entries: view.variables,
          },
          {
            title: t("sections.switches"),
            info: t("info.switches"),
            kind: "switch",
            entries: view.switches,
          },
        ]}
      />
    </div>
  );
}
