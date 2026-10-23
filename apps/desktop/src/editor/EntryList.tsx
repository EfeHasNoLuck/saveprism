import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { Entry } from "@/api/commands";
import { ENTRY_COLUMNS } from "./layout";
import { filterEntries } from "./helpers";
import {
  ColumnHeader,
  FieldRow,
  SectionTitle,
  TableNote,
  TablePanel,
  VirtualRows,
} from "./table";

export type EntryKind = "item" | "weapon" | "armor" | "variable" | "switch";

interface Props {
  title: string;
  info?: string;
  kind: EntryKind;
  entries: Entry[];
  /** Number of entries that differ from the file. */
  changed: number;
  query: string;
  onQuery: (query: string) => void;
}

/** One section of numbered entries as a dense table: #id, name, type, value, change, undo. */
export function EntryTable({ title, info, kind, entries, changed, query, onQuery }: Props) {
  const { t } = useTranslation("editor");
  const filtered = useMemo(() => filterEntries(entries, query), [entries, query]);

  return (
    <TablePanel>
      <SectionTitle
        title={title}
        info={info}
        total={entries.length}
        changed={changed}
        query={query}
        onQuery={onQuery}
        placeholder={t("search.list")}
      />
      <ColumnHeader
        columns={ENTRY_COLUMNS}
        labels={[
          t("values.id"),
          t("values.name"),
          t("values.type"),
          t("values.value"),
          t("values.change"),
          null,
        ]}
      />
      {entries.length === 0 ? (
        <TableNote>{t(`empty.${kind}`)}</TableNote>
      ) : filtered.length === 0 ? (
        <TableNote>{t("search.empty")}</TableNote>
      ) : (
        <VirtualRows
          label={title}
          items={filtered}
          itemKey={(entry) => String(entry.id)}
          render={(entry) => (
            <FieldRow
              id={entry.id}
              name={entry.name ?? t(`fallback.${kind}`, { id: entry.id })}
              field={entry.field}
              columns={ENTRY_COLUMNS}
            />
          )}
        />
      )}
    </TablePanel>
  );
}
