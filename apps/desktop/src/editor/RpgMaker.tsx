import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderSearch } from "lucide-react";
import { useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import type { Field, RpgMakerView } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Characters } from "./Characters";
import { useEditorContext } from "./context";
import { EntryList, type EntryListProps } from "./EntryList";
import { rootScale, useElementWidth } from "./grid";
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
          data-field-path={field.path}
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
  // In rem, so the cards grow with the Text size setting (the widths are pixels at 16px).
  if (columns === 0) return { flex: `${onlyEmpty ? 1 : 0} 1 ${cardWidth(0) / 16}rem` };
  return { flex: `${columns} 1 ${cardWidth(columns) / 16}rem` };
}

/**
 * A row of list cards. The entry columns that fit across the row go to the cards by their
 * length, so a long list gets more columns, an empty one stays narrow, and the cards end at
 * similar heights. The cards wrap under each other when the window is narrow.
 */
function ListRow({ lists }: { lists: Omit<EntryListProps, "style" | "className">[] }) {
  const { ref, width } = useElementWidth();
  const counts = lists.map((l) => l.entries.length);
  const columns = allocateColumns(width / rootScale(), counts);
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
 * Where the names come from: the folder found next to the save, or one the player picks by hand.
 * A long path is cut short; its tooltip holds the whole path.
 */
function DataFolderLine({ view }: { view: RpgMakerView }) {
  const { t } = useTranslation("editor");
  const { setDataFolder } = useEditorContext();
  const [picking, setPicking] = useState(false);

  async function choose() {
    if (picking) return;
    setPicking(true);
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: t("dataFolder.title"),
      });
      if (typeof picked === "string") await setDataFolder(picked);
    } finally {
      setPicking(false);
    }
  }

  return (
    <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <FolderSearch aria-hidden="true" className="size-3.5 shrink-0" />
      {view.namesFolder === null ? (
        <span className="truncate">{t("dataFolder.none")}</span>
      ) : (
        <span className="min-w-0 truncate font-mono text-[0.6875rem]" title={view.namesFolder}>
          {view.namesFolder}
        </span>
      )}
      <span aria-hidden="true">·</span>
      <Button
        variant="ghost"
        size="xs"
        className="shrink-0 text-xs"
        disabled={picking}
        onClick={() => void choose()}
      >
        {picking
          ? t("dataFolder.picking")
          : view.namesFolder === null
            ? t("dataFolder.choose")
            : t("dataFolder.change")}
      </Button>
    </div>
  );
}

/**
 * RPG Maker saves as cards in three rows: Gold and Characters; Items, Weapons and Armors;
 * Variables and Switches.
 */
export function RpgMaker({
  view,
  saveFolder,
}: {
  view: RpgMakerView;
  /** Folder of the open save; null for a pasted code, which has no game to keep names under. */
  saveFolder: string | null;
}) {
  const { t } = useTranslation("editor");
  const namesInfo = view.namesFolder === null ? t("info.noNames") : undefined;
  const namesGame = saveFolder === null ? null : (view.namesFolder ?? saveFolder);
  return (
    <div className="flex flex-col gap-2.5">
      <DataFolderLine view={view} />
      <div
        className={cn(
          "grid gap-2.5",
          view.gold === null ? "md:grid-cols-1" : "md:grid-cols-[14rem_minmax(0,1fr)]",
        )}
      >
        {view.gold === null ? null : <Gold field={view.gold} />}
        <Characters actors={view.actors} namesFolder={view.namesFolder} />
      </div>
      <ListRow
        lists={[
          {
            title: t("sections.items"),
            kind: "item",
            entries: view.items,
            info: namesInfo,
            canAdd: view.canAdd.items,
            catalog: view.catalog.items,
          },
          {
            title: t("sections.weapons"),
            kind: "weapon",
            entries: view.weapons,
            canAdd: view.canAdd.weapons,
            catalog: view.catalog.weapons,
          },
          {
            title: t("sections.armors"),
            kind: "armor",
            entries: view.armors,
            canAdd: view.canAdd.armors,
            catalog: view.catalog.armors,
          },
        ]}
      />
      <ListRow
        lists={[
          {
            title: t("sections.variables"),
            info: t("info.variables"),
            kind: "variable",
            entries: view.variables,
            namesGame,
          },
          {
            title: t("sections.switches"),
            info: t("info.switches"),
            kind: "switch",
            entries: view.switches,
            namesGame,
          },
        ]}
      />
    </div>
  );
}
