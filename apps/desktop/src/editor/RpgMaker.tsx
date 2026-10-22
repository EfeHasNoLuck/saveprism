import { useTranslation } from "react-i18next";
import type { RpgMakerView } from "@/api/commands";
import { Characters } from "./Characters";
import { EntryList } from "./EntryList";
import { FieldEditor } from "./FieldEditor";
import { Section } from "./parts";

/** Cards in three rows: Gold + Characters, Items + Weapons + Armors, Variables + Switches. */
export function RpgMaker({ view }: { view: RpgMakerView }) {
  const { t } = useTranslation("editor");
  return (
    <div className="flex flex-col gap-4">
      <Characters
        actors={view.actors}
        namesFolder={view.namesFolder}
        gold={
          <Section title={t("sections.gold")} info={t("info.gold")}>
            <FieldEditor field={view.gold} label={t("sections.gold")} alwaysHint />
          </Section>
        }
      />
      <div className="grid gap-4 lg:grid-cols-3">
        <EntryList
          title={t("sections.items")}
          kind="item"
          entries={view.items}
          info={view.namesFolder === null ? t("info.noNames") : undefined}
        />
        <EntryList title={t("sections.weapons")} kind="weapon" entries={view.weapons} />
        <EntryList title={t("sections.armors")} kind="armor" entries={view.armors} />
      </div>
      <div className="grid gap-4 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <EntryList
          title={t("sections.variables")}
          info={t("info.variables")}
          kind="variable"
          entries={view.variables}
          paged
        />
        <EntryList
          title={t("sections.switches")}
          info={t("info.switches")}
          kind="switch"
          entries={view.switches}
          paged
        />
      </div>
    </div>
  );
}
