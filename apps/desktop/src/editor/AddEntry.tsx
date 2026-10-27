import { Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { NamedId } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useEditorContext } from "./context";
import { catalogId, checkRange, filterCatalog } from "./helpers";

/** Most database entries the picker lists at once. */
const MAX_RESULTS = 40;

/** The game data file each list comes from (RPG Maker MV/MZ). */
const DATA_FILE: Record<"items" | "weapons" | "armors", string> = {
  items: "Items.json",
  weapons: "Weapons.json",
  armors: "Armors.json",
};

/**
 * "Add" of an inventory list: search the game's catalog by name or #id (or type an id when the
 * game's data folder was not found or its file is missing), a count from 1 to 99, then Add.
 */
export function AddEntryButton({
  list,
  catalog,
  folderFound,
  label,
}: {
  list: "items" | "weapons" | "armors";
  /** The game's entries for this list; empty when there is no data folder. */
  catalog: NamedId[];
  /** The game's data folder was found, even if this list's file was not. */
  folderFound: boolean;
  /** What is added ("item"), for the dialog texts. */
  label: string;
}) {
  const { t } = useTranslation("editor");
  const { addInventory } = useEditorContext();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<NamedId | null>(null);
  const [count, setCount] = useState("1");
  const [running, setRunning] = useState(false);

  const typedId = catalogId(query);
  const results = useMemo(
    () => filterCatalog(catalog, query).slice(0, MAX_RESULTS),
    [catalog, query],
  );
  const id = selected?.id ?? typedId;
  const countProblem = checkRange("int", count, 1, 99);
  const valid = id !== null && countProblem === null;

  function close(next: boolean) {
    setOpen(next);
    if (!next) {
      setQuery("");
      setSelected(null);
      setCount("1");
    }
  }

  async function submit() {
    if (!valid || id === null || running) return;
    setRunning(true);
    const ok = await addInventory(list, id, count.trim());
    setRunning(false);
    if (ok) close(false);
  }

  return (
    <Popover open={open} onOpenChange={close}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className="text-xs text-foreground-variant"
          title={t("addEntry.triggerTip", { label })}
        >
          <Plus />
          {t("addEntry.trigger", { label })}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80">
        <p className="text-[0.8125rem] font-semibold">{t("addEntry.title", { label })}</p>
        <form
          className="mt-2 flex flex-col gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            void submit();
          }}
        >
          {catalog.length > 0 ? (
            <>
              <div className="relative">
                <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                <input
                  type="search"
                  value={query}
                  autoFocus
                  aria-label={t("addEntry.search")}
                  placeholder={t("addEntry.search")}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setSelected(null);
                  }}
                  className="h-7 w-full rounded-sm border border-input/70 bg-field pr-2 pl-7 text-xs outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
                />
              </div>
              {selected !== null ? (
                <div className="flex items-center gap-1.5 rounded-sm bg-surface-high px-2 py-1 text-xs">
                  <span className="min-w-0 flex-1 truncate">
                    {selected.name}
                    <span className="ml-1 font-mono text-[0.6875rem] text-muted-foreground">
                      #{selected.id}
                    </span>
                  </span>
                  <Button type="button" variant="ghost" size="xs" onClick={() => setSelected(null)}>
                    {t("addEntry.clear")}
                  </Button>
                </div>
              ) : results.length > 0 ? (
                <ul className="dense-scroll flex max-h-48 flex-col gap-0.5 overflow-y-auto">
                  {results.map((entry) => (
                    <li key={entry.id}>
                      <button
                        type="button"
                        onClick={() => setSelected(entry)}
                        className="flex w-full items-center gap-2 rounded-sm px-2 py-1 text-left text-xs outline-none hover:bg-surface-container focus-visible:ring-2 focus-visible:ring-ring/60"
                      >
                        <span className="min-w-0 flex-1 truncate">{entry.name}</span>
                        <span className="shrink-0 font-mono text-[0.6875rem] text-muted-foreground">
                          #{entry.id}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-xs text-muted-foreground">{t("addEntry.noMatch")}</p>
              )}
              {typedId !== null && selected === null ? (
                <p className="text-[0.6875rem] text-muted-foreground">
                  {t("addEntry.typedId", { id: typedId })}
                </p>
              ) : null}
            </>
          ) : (
            <>
              <label className="flex flex-col gap-1 text-xs text-muted-foreground">
                {t("addEntry.id")}
                <input
                  type="text"
                  inputMode="numeric"
                  value={query}
                  autoFocus
                  aria-label={t("addEntry.id")}
                  placeholder={t("addEntry.idPlaceholder")}
                  onChange={(event) => setQuery(event.target.value)}
                  className="h-7 w-full rounded-sm border border-input/70 bg-field px-2 text-right font-mono text-xs tabular-nums outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
                />
              </label>
              <p className="text-[0.6875rem] text-muted-foreground">
                {folderFound
                  ? t("addEntry.missingFile", { file: DATA_FILE[list] })
                  : t("addEntry.noCatalog")}
              </p>
            </>
          )}
          <label className="flex items-center gap-2 text-xs text-muted-foreground">
            {t("addEntry.count")}
            <input
              type="text"
              inputMode="numeric"
              value={count}
              aria-label={t("addEntry.count")}
              aria-invalid={(countProblem !== null && countProblem !== "empty") || undefined}
              onChange={(event) => setCount(event.target.value)}
              className="h-7 w-16 rounded-sm border border-input/70 bg-field px-2 text-right font-mono text-xs tabular-nums outline-none focus-visible:border-ring aria-invalid:border-destructive dark:border-transparent dark:focus-visible:border-ring"
            />
            <span className="ml-auto font-mono text-[0.6875rem]">1–99</span>
          </label>
          {countProblem !== null && countProblem !== "empty" ? (
            <p role="alert" className="text-xs font-medium text-destructive">
              {t(`problem.${countProblem}`, { min: 1, max: 99 })}
            </p>
          ) : null}
          <Button type="submit" size="sm" disabled={!valid || running}>
            <Plus />
            {t("addEntry.submit")}
          </Button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
