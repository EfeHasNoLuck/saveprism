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
import { StateBar, UndoButton, ValueInput } from "./FieldEditor";
import { rootScale, useElementWidth } from "./grid";
import {
  buildCollection,
  buildDeck,
  collectionSummary,
  deckRows,
  factionCounts,
  filterCollection,
  groupByFaction,
  tileColumns,
  TILE_ROWS,
  type CollectionRow,
  type DeckRow,
  type FactionKey,
  type ShowFilter,
} from "./gwent";
import { pageCount, pageItems } from "./helpers";
import { EmptyText, Pager, SearchBox, Section } from "./parts";
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

/**
 * The deck on show (or the whole collection), its filters, search and page, and the rows they
 * leave.
 */
function useCollection(gwent: GwentView, cards: W3Card[] | null, pageSize: number) {
  const { t } = useTranslation("editor");
  const { reveal, fields, pending } = useEditorContext();
  /** The deck on show (1-based), or null for the whole collection. */
  const [deckChoice, setDeck] = useState<number | null>(null);
  const [factionChoice, setFaction] = useState<FactionKey | "all">("all");
  const [showChoice, setShow] = useState<ShowFilter>("all");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const chosenDeck = deckChoice === null ? undefined : gwent.decks[deckChoice - 1];
  const deck = chosenDeck === undefined ? null : deckChoice;
  // Without the game's list there are no factions and nothing missing to filter by; a deck has
  // no faction tabs.
  const faction = cards === null || deck !== null ? "all" : factionChoice;
  const show = cards === null ? "all" : showChoice;

  const collectionRows = useMemo(() => buildCollection(gwent.cards, cards), [gwent.cards, cards]);
  const rows = useMemo(
    () => (chosenDeck === undefined ? collectionRows : buildDeck(chosenDeck, gwent.cards, cards)),
    [cards, chosenDeck, collectionRows, gwent.cards],
  );
  const nameOf = useMemo(
    () => (row: CollectionRow) => row.name ?? t("gwent.fallback", { index: row.index }),
    [t],
  );
  const filtered = useMemo(
    () => filterCollection(rows, { faction, show, query }, nameOf),
    [faction, nameOf, query, rows, show],
  );
  const shown = pageItems(filtered, page, pageSize);

  // A compare row that is a copy count: back to the collection, drop the filters and turn to its
  // page.
  useEffect(() => {
    if (reveal === null) return;
    const index = collectionRows.findIndex((row) => row.copies?.path === reveal.path);
    if (index < 0) return;
    const handle = window.requestAnimationFrame(() => {
      setDeck(null);
      setFaction("all");
      setShow("all");
      setQuery("");
      setPage(Math.floor(index / pageSize));
    });
    return () => window.cancelAnimationFrame(handle);
  }, [reveal, collectionRows, pageSize]);

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
    collectionRows,
    filtered,
    shown,
    nameOf,
    deck,
    setDeck: (value: number | null) => {
      setDeck(value);
      setPage(0);
    },
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

/** A small uppercase mark ("Leader", "Selected") in the cyan accent, or muted ("Locked"). */
function Mark({ label, muted = false }: { label: string; muted?: boolean }) {
  return (
    <span
      className={cn(
        "shrink-0 rounded-sm px-1 text-[0.625rem] leading-4 font-semibold tracking-wide uppercase",
        muted ? "text-muted-foreground ring-1 ring-border" : "bg-tertiary/12 text-tertiary",
      )}
    >
      {label}
    </span>
  );
}

/** "Leader" after a leader card's name. */
function LeaderMark({ muted = false }: { muted?: boolean }) {
  const { t } = useTranslation("editor");
  return <Mark label={t("gwent.leader")} muted={muted} />;
}

/** The copies a deck holds (read-only), after the card's index when it shows one. */
function InDeck({ count, afterIndex }: { count: number | undefined; afterIndex: boolean }) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  if (count === undefined) return null;
  return (
    <>
      {afterIndex ? (
        <span aria-hidden="true" className="shrink-0 text-[0.6875rem] text-muted-foreground">
          ·
        </span>
      ) : null}
      <span className="min-w-0 truncate text-[0.6875rem] text-muted-foreground tabular-nums">
        {t("gwent.inDeck", { n: format(count) })}
      </span>
    </>
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
  const { cards, leaders } = collectionSummary(collection.collectionRows);
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
            <InDeck count={row.inDeck} afterIndex={row.name !== null} />
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
        {row.name !== null ? (
          <span className="shrink-0 font-mono tabular-nums">#{row.index}</span>
        ) : null}
        <InDeck count={row.inDeck} afterIndex={row.name !== null} />
        <span aria-hidden="true" className="min-w-1 flex-1" />
        <span className="shrink-0">{t("gwent.missing")}</span>
      </div>
    </div>
  );
}

/** A deck's tab: its faction (or number), card count and the Selected or Locked mark. */
function DeckOption({ deck, name }: { deck: DeckRow; name: string }) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  return (
    <span className="inline-flex items-center gap-1">
      {name}
      <span className="font-mono text-[0.6875rem] font-normal text-muted-foreground tabular-nums">
        {format(deck.cardCount)}
      </span>
      {deck.selected ? (
        <Mark label={t("gwent.selected")} />
      ) : !deck.unlocked ? (
        <Mark label={t("gwent.locked")} muted />
      ) : null}
    </span>
  );
}

/**
 * The Gwent card: the collection or one deck (tabs), the faction tabs of the collection, then the
 * cards as tiles in as many columns as fit, a page at a time.
 */
function GwentTiles({ gwent, catalog, picking, onPick }: GwentProps) {
  const { t } = useTranslation("editor");
  const format = useFormatNumber();
  const label = useFactionLabel();
  const { ref, width } = useElementWidth();
  const columns = tileColumns(width / rootScale());
  const pageSize = columns * TILE_ROWS;
  const cards = catalog.status === "ready" ? catalog.cards : null;
  const collection = useCollection(gwent, cards, pageSize);
  const counts = useMemo(
    () => factionCounts(collection.collectionRows),
    [collection.collectionRows],
  );
  const countMap = useMemo(
    () => new Map(counts.map((count) => [count.faction, count] as const)),
    [counts],
  );
  const decks = useMemo(
    () => deckRows(gwent.decks, cards, gwent.selected),
    [cards, gwent.decks, gwent.selected],
  );
  const deckName = (deck: DeckRow) =>
    deck.faction !== null ? label(deck.faction) : t("gwent.deck", { number: deck.number });
  const deck = collection.deck === null ? null : (decks[collection.deck - 1] ?? null);
  const deckCards = collection.deck === null ? null : gwent.decks[collection.deck - 1]?.cards;
  const title = t("gwent.title");
  const all = collectionSummary(collection.collectionRows);
  const groups = groupByFaction(collection.shown);
  const showHeadings = collection.faction === "all" && collection.deck === null && cards !== null;

  const deckTabs =
    decks.length > 0 ? (
      <Choice<string>
        label={t("gwent.view")}
        value={collection.deck === null ? "collection" : String(collection.deck)}
        onChange={(value) => collection.setDeck(value === "collection" ? null : Number(value))}
        options={[
          { value: "collection", label: t("gwent.collection") },
          ...decks.map((row) => ({
            value: String(row.number),
            label: <DeckOption deck={row} name={deckName(row)} />,
          })),
        ]}
      />
    ) : null;
  const factionTabs =
    collection.deck === null && counts.length > 0 ? (
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
    ) : null;
  // The deck tabs keep their place; the faction tabs sit under them (beside the summary when the
  // save has no decks) and leave while a deck is on show.
  const firstTabs = deckTabs ?? factionTabs;

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
          title={deck === null ? title : `${title} · ${deckName(deck)}`}
        />
      }
    >
      <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5">
        {firstTabs}
        <SummaryLine
          collection={collection}
          catalog={catalog}
          picking={picking}
          onPick={onPick}
          className={firstTabs !== null ? "ml-auto" : undefined}
        />
      </div>
      {deckTabs !== null && factionTabs !== null ? (
        <div className="flex min-w-0">{factionTabs}</div>
      ) : null}
      <div ref={ref} className="flex min-w-0 flex-col gap-2">
        {collection.shown.length > 0 ? (
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
        ) : deckCards?.length === 0 ? null : (
          <EmptyText>{t("gwent.empty")}</EmptyText>
        )}
        {deck !== null && deckCards?.length === 0 ? (
          <EmptyText>{deck.unlocked ? t("gwent.deckEmpty") : t("gwent.deckLocked")}</EmptyText>
        ) : null}
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

interface GwentProps {
  gwent: GwentView;
  catalog: Catalog;
  picking: boolean;
  onPick: () => void;
}

/** Witcher 3 saves: the Gwent collection and decks above the Values list of the whole save. */
export function Witcher3({ view }: { view: Witcher3View }) {
  const { catalog, picking, pick } = useCatalog();
  return (
    <div className="flex flex-col gap-2.5">
      {view.gwent === null ? null : (
        <GwentTiles
          gwent={view.gwent}
          catalog={catalog}
          picking={picking}
          onPick={() => void pick()}
        />
      )}
      <ValuesCard rows={view.rows} addParent={view.addParent} />
    </div>
  );
}
