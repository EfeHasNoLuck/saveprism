import { Check, ChevronDown } from "lucide-react";
import { Select } from "radix-ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  Entry,
  InventoryKind,
  SatisfactoryInventory,
  SatisfactoryView,
} from "@/api/commands";
import { EntryList } from "./EntryList";
import { EmptyText } from "./parts";
import { ValuesCard } from "./ValuesCard";
import { Choice } from "./Witcher3";

const KINDS: InventoryKind[] = ["player", "storage", "other"];

/** The player labels come from the backend in English; the UI translates them. */
function inventoryLabel(t: (key: string, options?: Record<string, unknown>) => string, inventory: SatisfactoryInventory): string {
  if (inventory.kind !== "player") return inventory.label;
  const match = /^Player (\d+) inventory$/.exec(inventory.label);
  if (match !== null) return t("satisfactory.playerNInventory", { n: match[1] });
  return t("satisfactory.playerInventory");
}

function InventorySelect({
  inventories,
  value,
  label,
  onChange,
}: {
  inventories: SatisfactoryInventory[];
  value: string;
  label: string | undefined;
  onChange: (id: string) => void;
}) {
  const { t } = useTranslation("editor");
  if (inventories.length <= 1) return null;
  return (
    <Select.Root value={value} onValueChange={onChange}>
      <Select.Trigger
        aria-label={t("satisfactory.pick")}
        className="inline-flex h-7 max-w-64 min-w-0 items-center gap-1.5 rounded-sm border border-input/70 bg-field px-2 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
      >
        <span className="min-w-0 truncate">{label}</span>
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
            {inventories.map((inventory) => (
              <Select.Item
                key={inventory.id}
                value={inventory.id}
                className="relative flex h-7 cursor-default items-center gap-3 rounded-sm pr-2 pl-6 text-xs outline-none select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground"
              >
                <Select.ItemIndicator className="absolute left-1.5 inline-flex">
                  <Check className="size-3.5" />
                </Select.ItemIndicator>
                <Select.ItemText>
                  {inventoryLabel((key, options) => t(key, options), inventory)}
                </Select.ItemText>
                <span className="ml-auto font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
                  {inventory.stacks.length}
                </span>
              </Select.Item>
            ))}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}

/**
 * Satisfactory saves: a picker of the non-empty inventories (player, storage and other tabs),
 * the picked inventory's item stacks with editable counts, and below them the raw Values list.
 */
export function Satisfactory({ view }: { view: SatisfactoryView }) {
  const { t } = useTranslation("editor");
  const [kindChoice, setKind] = useState<InventoryKind>(() => {
    return KINDS.find((kind) => view.inventories.some((inv) => inv.kind === kind)) ?? "player";
  });
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const counts = useMemo(() => {
    const map = new Map<InventoryKind, number>();
    for (const kind of KINDS) {
      map.set(kind, view.inventories.filter((inv) => inv.kind === kind).length);
    }
    return map;
  }, [view.inventories]);
  const available = KINDS.filter((kind) => (counts.get(kind) ?? 0) > 0);
  const kind = counts.get(kindChoice) ? kindChoice : (available[0] ?? "player");
  const inventories = view.inventories.filter((inventory) => inventory.kind === kind);
  const selected =
    inventories.find((inventory) => inventory.id === selectedId) ?? inventories[0];
  const label =
    selected === undefined
      ? undefined
      : inventoryLabel((key, options) => t(key, options), selected);
  const entries: Entry[] = (selected?.stacks ?? []).map((stack, index) => ({
    id: index + 1,
    name: stack.name,
    field: stack.count,
    removable: false,
  }));

  const tabs =
    available.length <= 1 ? null : (
      <Choice<InventoryKind>
        label={t("satisfactory.kinds")}
        value={kind}
        onChange={setKind}
        options={available.map((option) => ({
          value: option,
          label: (
            <>
              {t(`satisfactory.kind.${option}`)}{" "}
              <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
                {counts.get(option) ?? 0}
              </span>
            </>
          ),
        }))}
      />
    );

  return (
    <div className="flex flex-col gap-2.5">
      {view.inventories.length === 0 ? (
        <EmptyText>{t("satisfactory.empty")}</EmptyText>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-2 px-1">
            {tabs}
            {selected === undefined ? null : (
              <InventorySelect
                inventories={inventories}
                value={selected.id}
                label={label}
                onChange={setSelectedId}
              />
            )}
          </div>
          {selected === undefined ? (
            <EmptyText>{t("satisfactory.empty")}</EmptyText>
          ) : (
            <EntryList
              key={selected.id}
              title={label ?? selected.label}
              info={t("satisfactory.info")}
              kind="item"
              entries={entries}
            />
          )}
        </>
      )}
      <ValuesCard rows={view.rows} addParent={view.addParent} />
    </div>
  );
}
