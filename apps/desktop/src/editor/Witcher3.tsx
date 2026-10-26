import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { w3Cards, type Field, type GwentView, type W3Card, type Witcher3View } from "@/api/commands";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { AskAiButton } from "./AskAiButton";
import type { AskAiRow } from "./askAi";
import { countPending, useEditorContext, useFormatNumber } from "./context";
import { FieldCell, StateBar, UndoButton, ValueInput } from "./FieldEditor";
import { rootScale, useElementWidth, useGridColumns } from "./grid";
import {
  buildCollection,
  collectionSummary,
  deckRows,
  factionCounts,
  factionKey,
  filterCollection,
  groupByFaction,
  GWENT_LAYOUT,
  tileColumns,
  TILE_ROWS,
  type CollectionRow,
  type FactionKey,
  type ShowFilter,
} from "./gwent";
import { pageCount, pageItems, ROWS_PER_PAGE } from "./helpers";
import { DenseGrid, EmptyText, Pager, SearchBox, Section } from "./parts";
import { fieldState, useFieldEdit } from "./useFieldEdit";
import { ValuesCard } from "./ValuesCard";

/** The game's card list: still loading, read from the game's files, or not found. */
type Catalog =
  | { status: "loading" }
  | { status: "ready"; cards: W3Card[] }
  | { status: "missing"; failed: boolean };

/**
 * The card names from the player's own game files. The game folder is looked for without asking;
 * when none is found the player can pick one.
 */
function useCatalog() {
  const { t } = useTranslation("editor");
  const [catalog, setCatalog] = useState<Catalog>({ status: "loading" });
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    let cancelled = false;
    w3Cards().then(
      (view) => {
        if (cancelled) return;
        setCatalog(
          view.state === "ready"
            ? { status: "ready", cards: view.cards }
            : { status: "missing", failed: false },
        );
      },
      () => {
        if (!cancelled) setCatalog({ status: "missing", failed: false });
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  async function pick() {
    if (picking) return;
    setPicking(true);
    try {
      const folder = await openDialog({
        directory: true,
        multiple: false,
        title: t("gwent.pickTitle"),
      });
      if (typeof folder !== "string") return;
      const view = await w3Cards(folder);
      setCatalog(
        view.state === "ready"
          ? { status: "ready", cards: view.cards }
          : { status: "missing", failed: true },
      );
    } catch {
      setCatalog({ status: "missing", failed: true });
    } finally {
      setPicking(false);
    }
  }

  return { catalog, picking, pick };
}

/** The filters, search and page of the collection, and the rows they leave. */
function useCollection(gwent: GwentView, cards: W3Card[] | null, pageSize: number) {
  const { t } = useTranslation("editor");
  const { reveal, fields, pending } = useEditorContext();
  const [factionChoice, setFaction] = useState<FactionKey | "all">("all");
  const [showChoice, setShow] = useState<ShowFilter>("all");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  // Without the game's list there are no factions and nothing missing to filter by.
  const faction = cards === null ? "all" : factionChoice;
  const show = cards === null ? "all" : showChoice;

  const rows = useMemo(() => buildCollection(gwent.cards, cards), [gwent.cards, cards]);
  const nameOf = useMemo(
    () => (row: CollectionRow) => row.name ?? t("gwent.fallback", { index: row.index }),
    [t],
  );
  const filtered = useMemo(
    () => filterCollection(rows, { faction, show, query }, nameOf),
    [faction, nameOf, query, rows, show],
  );
  const shown = pageItems(filtered, page, pageSize);

  // A compare row that is a copy count: drop the filters and turn to its page.
  useEffect(() => {
    if (reveal === null) return;
    const index = rows.findIndex((row) => row.copies?.path === reveal.path);
    if (index < 0) return;
    const handle = window.requestAnimationFrame(() => {
      setFaction("all");
      setShow("all");
      setQuery("");
      setPage(Math.floor(index / pageSize));
    });
    return () => window.cancelAnimationFrame(handle);
  }, [reveal, rows, pageSize]);

  const ownedPaths = useMemo(
    () => gwent.cards.map((card) => card.copies.path),
    [gwent.cards],
  );
  const changed = countPending(pending, ownedPaths);
  // The values "Ask AI" copies: the owned cards the filters leave, on every page.
  const askRows = useMemo<AskAiRow[]>(
    () =>
      filtered.flatMap((row) => {
        if (row.copies === null) return [];
        const field = fields.get(row.copies.path) ?? row.copies;
        return [
          {
            name: nameOf(row),
            id: row.index,
            path: field.path,
            value: field.value,
            kind: field.kind,
            min: field.min,
            max: field.max,
          },
        ];
      }),
    [fields, filtered, nameOf],
  );

  return {
    rows,
    filtered,
    shown,
    nameOf,
    faction,
    setFaction: (value: FactionKey | "all") => {
      setFaction(value);
      setPage(0);
    },
    show,
    setShow: (value: ShowFilter) => {
      setShow(value);
      setPage(0);
    },
    query,
    setQuery: (value: string) => {
      setQuery(value);
      setPage(0);
    },
    page,
    setPage,
    changed,
    askRows,
  };
}

type Collection = ReturnType<typeof useCollection>;

/** A small radio group in the Settings style: the chosen option is tinted blue. */
function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: ReactNode }[];
  onChange: (value: T) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex shrink-0 rounded-sm bg-field p-0.5 ring-1 ring-border"
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className={cn(
            "rounded-[3px] px-2 py-0.5 text-xs whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            option.value === value
              ? "bg-primary/15 font-semibold text-primary"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

/** "Leader" after a leader card's name, in the cyan accent. */
function LeaderMark({ muted = false }: { muted?: boolean }) {
  const { t } = useTranslation("editor");
  return (
    <span
      className={cn(
        "shrink-0 rounded-sm px-1 text-[0.625rem] leading-4 font-semibold tracking-wide uppercase",
        muted ? "text-muted-foreground ring-1 ring-border" : "bg-tertiary/12 text-tertiary",
      )}
    >
      {t("gwent.leader")}
    </span>
  );
}

function useFactionLabel() {
  const { t } = useTranslation("editor");
  return (faction: FactionKey) => t(`gwent.faction.${faction}`);
}

/**
 * The quiet line under the header: how many cards the save holds (of how many the game has) and,
 * without the game's files, the one action that brings the names and missing cards.
 */
function SummaryLine({
  collection,
  catalog,
  picking,
  onPick,
  className,
}: {
  collection: Collection;
  catalog: Catalog;
  picking: boolean;
  onPick: () => void;
  className?: string;
}) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const { cards, leaders } = collectionSummary(collection.rows);
  const ready = catalog.status === "ready";
  const parts = ready
    ? [
        t("gwent.cardsOf", { owned: format(cards.owned), total: format(cards.total) }),
        t("gwent.leadersOf", { owned: format(leaders.owned), total: format(leaders.total) }),
      ]
    : [
        t("gwent.cards", { count: cards.owned, n: format(cards.owned) }),
        t("gwent.leaders", { count: leaders.owned, n: format(leaders.owned) }),
      ];
  return (
    <div
      className={cn(
        "flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 text-xs text-muted-foreground tabular-nums",
        className,
      )}
    >
      <span>{parts.join(" · ")}</span>
      {catalog.status === "missing" ? (
        <>
          <span aria-hidden="true">·</span>
          <Button
            variant="ghost"
            size="xs"
            className="-ml-1 text-xs text-primary"
            title={t("gwent.pickFolderTip")}
            disabled={picking}
            onClick={onPick}
          >
            {picking ? t("gwent.picking") : t("gwent.pickFolder")}
          </Button>
          {catalog.failed ? (
            <span role="alert" className="text-destructive">
              {t("gwent.pickFailed")}
            </span>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

/** Search, the Owned/Missing filter and the pager, for the card header. */
function HeaderControls({
  collection,
  catalog,
  pageSize,
  title,
}: {
  collection: Collection;
  catalog: Catalog;
  pageSize: number;
  title: string;
}) {
  const { t } = useTranslation("editor");
  return (
    <>
      <SearchBox
        value={collection.query}
        onChange={collection.setQuery}
        placeholder={t("gwent.search")}
      />
      {catalog.status === "ready" ? (
        <Choice<ShowFilter>
          label={t("gwent.show")}
          value={collection.show}
          onChange={collection.setShow}
          options={[
            { value: "all", label: t("gwent.showAll") },
            { value: "owned", label: t("gwent.showOwned") },
            { value: "missing", label: t("gwent.showMissing") },
          ]}
        />
      ) : null}
      {pageCount(collection.filtered.length, pageSize) > 1 ? (
        <Pager
          page={collection.page}
          total={collection.filtered.length}
          size={pageSize}
          onPage={collection.setPage}
        />
      ) : null}
      <AskAiButton
        list={title}
        query={collection.query}
        rows={collection.askRows}
        total={collection.askRows.length}
      />
    </>
  );
}

/** A faction's name and its owned/listed count, above its cards. */
function GroupHeading({
  faction,
  counts,
  className,
}: {
  faction: FactionKey;
  counts: Map<FactionKey, { owned: number; total: number }>;
  className?: string;
}) {
  const format = useFormatNumber();
  const label = useFactionLabel();
  const count = counts.get(faction);
  return (
    <h3
      className={cn(
        "flex items-baseline gap-1.5 text-xs font-semibold text-foreground-variant",
        className,
      )}
    >
      {label(faction)}
      {count !== undefined ? (
        <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
          {format(count.owned)} / {format(count.total)}
        </span>
      ) : null}
    </h3>
  );
}

// Layout A: tiles.

/**
 * An owned card as a tile: its name (and the leader mark) on top, the index, "old → new" when
 * changed, and the copy count with − and + below. The type and limits show while the box has focus.
 */
function OwnedTile({ row, name, field }: { row: CollectionRow; name: string; field: Field }) {
  const { t } = useTranslation("editor");
  const edit = useFieldEdit(field);
  const state = fieldState(edit);
  const floating = edit.focused || edit.invalid;
  const hint =
    edit.invalid && edit.message !== null
      ? edit.message
      : edit.changed
        ? `${edit.typeHint} · ${t("gwent.was", { value: edit.originalText })}`
        : edit.typeHint;
  return (
    <Tooltip open={floating}>
      <TooltipTrigger asChild>
        <div
          data-field-path={field.path}
          className={cn(
            "relative flex min-w-0 flex-col gap-0.5 rounded-sm border py-1.5 pr-1 pl-2.5",
            state === "idle" && "border-border bg-surface-container/50",
            state === "changed" && "border-changed/30 bg-changed-row",
            state === "error" && "border-destructive/40 bg-error-row",
          )}
          title={edit.disabled ? name : undefined}
        >
          <StateBar state={state} />
          <div className="flex h-5 min-w-0 items-center gap-1.5 pr-1.5">
            <span className="min-w-0 truncate text-xs font-medium" title={name}>
              {name}
            </span>
            {row.leader ? <LeaderMark /> : null}
            {edit.change !== null ? (
              <span className="ml-auto shrink-0 font-mono text-[0.6875rem] text-changed-foreground tabular-nums">
                {edit.change}
              </span>
            ) : null}
          </div>
          <div className="flex h-6.5 min-w-0 items-center gap-1">
            {/* An unnamed card's fallback name already carries the number. */}
            {row.name !== null ? (
              <span className="shrink-0 font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
                #{row.index}
              </span>
            ) : null}
            <span aria-hidden="true" className="min-w-1 flex-1" />
            <ValueInput
              edit={edit}
              label={t("gwent.copies", { name })}
              steppers
              className="shrink-0"
              inputClassName="w-auto min-w-9 max-w-24 field-sizing-content"
            />
            <UndoButton edit={edit} />
          </div>
        </div>
      </TooltipTrigger>
      {floating ? (
        <TooltipContent
          side="bottom"
          align="end"
          sideOffset={2}
          className={cn("max-w-72", edit.invalid && "font-medium text-destructive")}
        >
          <span role={edit.invalid ? "alert" : undefined}>{hint}</span>
        </TooltipContent>
      ) : null}
    </Tooltip>
  );
}

/** A card the save does not hold: dashed and muted, with no number box. */
function MissingTile({ row, name }: { row: CollectionRow; name: string }) {
  const { t } = useTranslation("editor");
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-sm border border-dashed border-border py-1.5 pr-2 pl-2.5 text-muted-foreground">
      <div className="flex h-5 min-w-0 items-center gap-1.5">
        <span className="min-w-0 truncate text-xs" title={name}>
          {name}
        </span>
        {row.leader ? <LeaderMark muted /> : null}
      </div>
      <div className="flex h-6.5 min-w-0 items-center gap-1 text-[0.6875rem]">
        <span className="shrink-0 font-mono tabular-nums">#{row.index}</span>
        <span aria-hidden="true" className="min-w-1 flex-1" />
        <span className="shrink-0">{t("gwent.missing")}</span>
      </div>
    </div>
  );
}

/** Layout A: faction tabs, then the cards as tiles in as many columns as fit, a page at a time. */
function GwentTiles({ gwent, catalog, picking, onPick }: GwentLayoutProps) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const label = useFactionLabel();
  const { ref, width } = useElementWidth();
  const columns = tileColumns(width / rootScale());
  const pageSize = columns * TILE_ROWS;
  const cards = catalog.status === "ready" ? catalog.cards : null;
  const collection = useCollection(gwent, cards, pageSize);
  const counts = useMemo(() => factionCounts(collection.rows), [collection.rows]);
  const countMap = useMemo(
    () => new Map(counts.map((count) => [count.faction, count] as const)),
    [counts],
  );
  const title = t("gwent.title");
  const all = collectionSummary(collection.rows);
  const groups = groupByFaction(collection.shown);
  const showHeadings = collection.faction === "all" && cards !== null;

  return (
    <Section
      title={title}
      changed={collection.changed}
      info={t("gwent.info")}
      aside={
        <HeaderControls
          collection={collection}
          catalog={catalog}
          pageSize={pageSize}
          title={title}
        />
      }
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5">
        {counts.length > 0 ? (
          <Choice<FactionKey | "all">
            label={t("gwent.factions")}
            value={collection.faction}
            onChange={collection.setFaction}
            options={[
              {
                value: "all",
                label: (
                  <>
                    {t("gwent.allFactions")}{" "}
                    <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
                      {format(all.cards.owned + all.leaders.owned)}
                    </span>
                  </>
                ),
              },
              ...counts.map((count) => ({
                value: count.faction,
                label: (
                  <>
                    {label(count.faction)}{" "}
                    <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
                      {format(count.owned)}
                    </span>
                  </>
                ),
              })),
            ]}
          />
        ) : null}
        <SummaryLine
          collection={collection}
          catalog={catalog}
          picking={picking}
          onPick={onPick}
          className={counts.length > 0 ? "ml-auto" : undefined}
        />
      </div>
      <div ref={ref} className="min-w-0">
        {collection.shown.length === 0 ? (
          <EmptyText>{t("gwent.empty")}</EmptyText>
        ) : (
          <div
            className="grid gap-2"
            style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}
          >
            {groups.map((group, index) => (
              <TileGroup
                key={`${group.faction ?? "none"}-${index}`}
                rows={group.rows}
                heading={
                  showHeadings && group.faction !== null ? (
                    <GroupHeading
                      faction={group.faction}
                      counts={countMap}
                      className={cn("col-span-full", index > 0 && "pt-1.5")}
                    />
                  ) : null
                }
                nameOf={collection.nameOf}
              />
            ))}
          </div>
        )}
      </div>
    </Section>
  );
}

function TileGroup({
  rows,
  heading,
  nameOf,
}: {
  rows: CollectionRow[];
  heading: ReactNode;
  nameOf: (row: CollectionRow) => string;
}) {
  return (
    <>
      {heading}
      {rows.map((row) =>
        row.copies === null ? (
          <MissingTile key={`missing-${row.index}`} row={row} name={nameOf(row)} />
        ) : (
          <OwnedTile key={row.copies.path} row={row} name={nameOf(row)} field={row.copies} />
        ),
      )}
    </>
  );
}

// Layout B: a dense list and the decks.

/** A card the save does not hold, as a muted row of the dense list. */
function MissingRow({ row, name }: { row: CollectionRow; name: string }) {
  const { t } = useTranslation("editor");
  return (
    <div className="flex h-7 min-w-0 items-center gap-1 rounded-sm pr-2 pl-2 text-xs text-muted-foreground">
      <span className="min-w-14 truncate" title={name}>
        {name}
      </span>
      {row.leader ? <LeaderMark muted /> : null}
      <span className="shrink-0 font-mono text-[0.6875rem] tabular-nums">#{row.index}</span>
      <span aria-hidden="true" className="min-w-1 flex-1" />
      <span className="shrink-0 text-[0.6875rem]">{t("gwent.missing")}</span>
    </div>
  );
}

/** Layout B: the cards grouped by faction in dense columns, a page at a time. */
function GwentList({ gwent, catalog, picking, onPick }: GwentLayoutProps) {
  const { t } = useTranslation("editor");
  const { ref, columns } = useGridColumns();
  const pageSize = columns * ROWS_PER_PAGE;
  const cards = catalog.status === "ready" ? catalog.cards : null;
  const collection = useCollection(gwent, cards, pageSize);
  const counts = useMemo(
    () => new Map(factionCounts(collection.rows).map((count) => [count.faction, count] as const)),
    [collection.rows],
  );
  const title = t("gwent.title");
  const groups = groupByFaction(collection.shown);

  return (
    <div className="grid items-start gap-2.5 lg:grid-cols-[minmax(0,1fr)_16rem]">
      <Section
        title={title}
        changed={collection.changed}
        info={t("gwent.info")}
        aside={
          <HeaderControls
            collection={collection}
            catalog={catalog}
            pageSize={pageSize}
            title={title}
          />
        }
      >
        <SummaryLine collection={collection} catalog={catalog} picking={picking} onPick={onPick} />
        <div ref={ref} className="flex min-w-0 flex-col gap-2">
          {collection.shown.length === 0 ? (
            <EmptyText>{t("gwent.empty")}</EmptyText>
          ) : (
            groups.map((group, index) => (
              <div key={`${group.faction ?? "none"}-${index}`} className="flex flex-col gap-1">
                {group.faction !== null ? (
                  <GroupHeading faction={group.faction} counts={counts} className="pl-2" />
                ) : null}
                <DenseGrid columns={columns} count={group.rows.length}>
                  {group.rows.map((row) => {
                    const name = collection.nameOf(row);
                    return row.copies === null ? (
                      <MissingRow key={`missing-${row.index}`} row={row} name={name} />
                    ) : (
                      <FieldCell
                        key={row.copies.path}
                        field={row.copies}
                        name={name}
                        id={row.name !== null ? row.index : undefined}
                        nameNode={
                          row.leader ? (
                            <>
                              <span className="min-w-14 truncate" title={name}>
                                {name}
                              </span>
                              <LeaderMark />
                            </>
                          ) : undefined
                        }
                      />
                    );
                  })}
                </DenseGrid>
              </div>
            ))
          )}
        </div>
      </Section>
      <DecksCard gwent={gwent} cards={cards} />
    </div>
  );
}

/** The five decks (read-only): faction or number, leader, card count; the selected one in cyan. */
function DecksCard({ gwent, cards }: { gwent: GwentView; cards: W3Card[] | null }) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const label = useFactionLabel();
  const decks = deckRows(gwent.decks, cards, gwent.selected);
  const selected = factionKey(gwent.selected);
  return (
    <Section title={t("gwent.decks")} count={format(decks.length)} info={t("gwent.decksInfo")}>
      {selected !== null ? (
        <div className="flex h-6 min-w-0 items-center gap-3 text-xs">
          <span className="shrink-0 text-muted-foreground">{t("gwent.selectedFaction")}</span>
          <span className="ml-auto min-w-0 truncate font-medium text-tertiary">
            {label(selected)}
          </span>
        </div>
      ) : null}
      <ul className="flex flex-col gap-1">
        {decks.map((deck) => {
          const leader =
            deck.leaderName ??
            (deck.leader === null
              ? t("gwent.noLeader")
              : t("gwent.fallback", { index: deck.leader }));
          return (
            <li
              key={deck.number}
              className={cn(
                "relative flex flex-col rounded-sm py-1 pr-2 pl-2.5",
                deck.selected ? "bg-surface-container" : "hover:bg-surface-container/60",
              )}
            >
              {deck.selected ? (
                <span
                  aria-hidden="true"
                  className="absolute inset-y-0 left-0 w-[3px] rounded-l-sm bg-tertiary"
                />
              ) : null}
              <div className="flex h-5 min-w-0 items-center gap-2 text-xs">
                <span
                  className={cn(
                    "min-w-0 truncate font-medium",
                    !deck.unlocked && "text-muted-foreground",
                  )}
                >
                  {deck.faction !== null
                    ? label(deck.faction)
                    : t("gwent.deck", { number: deck.number })}
                </span>
                <span className="ml-auto shrink-0 text-[0.6875rem]">
                  {deck.selected ? (
                    <span className="font-medium text-tertiary">{t("gwent.selected")}</span>
                  ) : !deck.unlocked ? (
                    <span className="text-muted-foreground">{t("gwent.locked")}</span>
                  ) : null}
                </span>
              </div>
              <div className="flex h-5 min-w-0 items-center gap-2 text-[0.6875rem] text-muted-foreground">
                <span className="min-w-0 truncate" title={leader}>
                  {leader}
                </span>
                <span className="ml-auto shrink-0 tabular-nums">
                  {t("gwent.deckCards", { count: deck.cardCount, n: format(deck.cardCount) })}
                </span>
              </div>
            </li>
          );
        })}
      </ul>
    </Section>
  );
}

interface GwentLayoutProps {
  gwent: GwentView;
  catalog: Catalog;
  picking: boolean;
  onPick: () => void;
}

/** Witcher 3 saves: the Gwent collection above the Values list of the whole save. */
export function Witcher3({ view }: { view: Witcher3View }) {
  const { catalog, picking, pick } = useCatalog();
  const props = { catalog, picking, onPick: () => void pick() };
  return (
    <div className="flex flex-col gap-2.5">
      {view.gwent === null ? null : GWENT_LAYOUT === "a" ? (
        <GwentTiles gwent={view.gwent} {...props} />
      ) : (
        <GwentList gwent={view.gwent} {...props} />
      )}
      <ValuesCard rows={view.rows} addParent={view.addParent} />
    </div>
  );
}
