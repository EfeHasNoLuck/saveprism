import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Bug, ExternalLink, Folder, FolderOpen, Info, Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { openSaveLocation, saveLocations, type SaveLocation } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { cn } from "@/lib/utils";
import { useNotify } from "./context";
import { describeError } from "./errors";
import {
  ENGINE_GROUPS,
  engineInitials,
  filterLocations,
  highlightText,
  isRegistryPath,
  tokenizePath,
  translatePathPlaceholders,
  type PlaceholderLabels,
} from "./locations";

const ISSUE_URL = "https://github.com/huseyincancalti/saveprism/issues/new";

type LocationsState =
  | { status: "loading" }
  | { status: "ready"; locations: SaveLocation[] }
  | { status: "error"; message: string };

interface SaveLocationsDialogProps {
  onClose: () => void;
}

/**
 * "Where is my save?": the usual save folder of each engine, grouped by engine and searchable.
 * SavePrism never scans the disk; the host only checks the returned candidates.
 */
export function SaveLocationsDialog({ onClose }: SaveLocationsDialogProps) {
  const { t } = useTranslation();
  const { notify } = useNotify();
  const [state, setState] = useState<LocationsState>({ status: "loading" });
  const [query, setQuery] = useState("");
  const [group, setGroup] = useState("all");
  const searchRef = useRef<HTMLInputElement>(null);
  // The host sends `<game folder>`, `<game>` and `<company>` as they are; only the display text is
  // translated, so a path the player copies or opens is unchanged.
  const labels: PlaceholderLabels = {
    gameFolder: t("locations.placeholder.gameFolder"),
    game: t("locations.placeholder.game"),
    company: t("locations.placeholder.company"),
  };

  /** Opens a folder in Explorer; a refusal is reported as a toast. */
  function openFolder(folder: string) {
    if (folder === "") return;
    void openSaveLocation(folder).catch((error: unknown) => {
      const { reason, fix } = describeError(error, t);
      notify({
        kind: "error",
        title: t("locations.openFailed"),
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    });
  }

  /** Opens the new-issue page in the browser. */
  function tellUs() {
    void openUrl(ISSUE_URL).catch((error: unknown) => {
      const { reason, fix } = describeError(error, t);
      notify({
        kind: "error",
        title: t("locations.tellUsFailed"),
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    });
  }

  useEffect(() => {
    let cancelled = false;
    saveLocations()
      .then((locations) => !cancelled && setState({ status: "ready", locations }))
      .catch((error: unknown) => {
        if (cancelled) return;
        const { reason, fix } = describeError(error, t);
        setState({
          status: "error",
          message: `${t("locations.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  // Ctrl+F belongs to this dialog while it is open: jump to the search box.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        (event.ctrlKey || event.metaKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "f"
      ) {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const items = useMemo(() => {
    if (state.status !== "ready") return [];
    return state.locations.map((location) => ({
      engine: location.engine,
      name: t(`locations.engine.${location.engine}`),
      hint: t(`locations.hint.${location.engine}`),
      paths: location.paths,
    }));
  }, [state, t]);

  const shown = filterLocations(items, query, group);
  const searching = query.trim() !== "";

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="h-[min(704px,calc(100vh-4rem))] w-[min(920px,calc(100vw-3rem))] max-w-[920px] gap-0 overflow-hidden p-0">
        <DialogHeader className="px-6 pt-5 pr-14">
          <DialogTitle>{t("locations.title")}</DialogTitle>
          <DialogDescription className="mt-0.5">{t("locations.description")}</DialogDescription>
        </DialogHeader>

        <div
          className={cn(
            "mx-6 mt-3.5 flex h-[38px] items-center gap-2.5 rounded-md border bg-field pr-2 pl-3",
            searching ? "border-primary ring-3 ring-primary/20" : "border-input",
          )}
        >
          <Search className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
          <label htmlFor="save-locations-search" className="sr-only">
            {t("locations.searchLabel")}
          </label>
          <input
            id="save-locations-search"
            ref={searchRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("locations.searchPlaceholder")}
            className="min-w-0 flex-1 bg-transparent text-[13.5px] outline-none placeholder:text-muted-foreground"
            autoComplete="off"
            spellCheck={false}
          />
          {state.status === "ready" ? (
            <span className="shrink-0 text-xs text-muted-foreground">
              {t("locations.count", { shown: shown.length, total: items.length })}
            </span>
          ) : null}
          {searching ? (
            <button
              type="button"
              onClick={() => setQuery("")}
              aria-label={t("locations.clearSearch")}
              title={t("locations.clearSearch")}
              className="rounded-sm p-1 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <X className="size-3.5" aria-hidden="true" />
            </button>
          ) : null}
        </div>

        <div className="mx-6 mt-3 flex flex-wrap gap-1.5 border-b border-border pb-2.5">
          <FilterChip active={group === "all"} onClick={() => setGroup("all")}>
            {t("locations.all")}
          </FilterChip>
          {[...ENGINE_GROUPS, { id: "other", engines: [] }].map((entry) => (
            <FilterChip
              key={entry.id}
              active={group === entry.id}
              onClick={() => setGroup(entry.id)}
            >
              {t(`locations.group.${entry.id}`)}
            </FilterChip>
          ))}
        </div>

        <div className="dense-scroll flex-1 overflow-y-auto px-6 pt-3.5 pb-3">
          {state.status === "loading" ? (
            <p className="py-4 text-center text-[0.8125rem] text-muted-foreground">
              {t("common.loading")}
            </p>
          ) : state.status === "error" ? (
            <p role="alert" className="py-2 text-[0.8125rem] text-destructive">
              {state.message}
            </p>
          ) : shown.length === 0 ? (
            <div className="mt-2 flex items-start gap-2.5 rounded-md border border-border bg-card px-3 py-3 text-[0.8125rem] text-muted-foreground">
              <Info className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
              <div>
                <p className="font-medium text-foreground">{t("locations.empty")}</p>
                <p className="mt-0.5">{t("locations.emptyHint")}</p>
              </div>
            </div>
          ) : (
            shown.map((item) => (
              <section
                key={item.engine}
                className="mb-2.5 overflow-hidden rounded-md border border-border bg-card dark:border-[#1d2d42]"
              >
                <div className="flex items-center gap-3 px-3.5 py-2.5">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface-container font-mono text-[11.5px] font-bold tracking-tight text-tertiary dark:bg-surface-high">
                    {engineInitials(item.engine)}
                  </span>
                  <div className="min-w-0">
                    <b className="block text-sm leading-[19px] font-semibold">
                      <Highlight text={item.name} query={query} />
                    </b>
                    <span className="mt-px block text-xs leading-4 text-muted-foreground">
                      <Highlight text={item.hint} query={query} />
                    </span>
                  </div>
                  <span className="ml-auto shrink-0 text-[11.5px] text-muted-foreground">
                    {item.paths.length === 0
                      ? t("locations.inBrowser")
                      : t("locations.folders", { count: item.paths.length })}
                  </span>
                </div>
                {item.paths.map((path) => {
                  const display = translatePathPlaceholders(path.shown, labels);
                  const registry = isRegistryPath(path.shown);
                  const relative = path.shown.startsWith("<game folder>");
                  return (
                    <div
                      key={path.shown}
                      className="flex min-h-10 items-center gap-3 border-t border-border py-1 pr-3.5 pl-[58px] dark:border-[#1d2d42]"
                    >
                      <span
                        className="min-w-0 flex-1 truncate font-mono text-xs"
                        title={display}
                      >
                        <PathText text={display} query={query} />
                      </span>
                      {registry ? (
                        <span className="flex shrink-0 items-center gap-2">
                          <span className="rounded-sm bg-surface-container px-1.5 py-px text-[10.5px] font-semibold tracking-wide text-foreground-variant uppercase dark:bg-surface-high">
                            {t("locations.registry")}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {t("locations.registryHint")}
                          </span>
                        </span>
                      ) : relative ? (
                        <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
                          <Folder className="size-3.5" aria-hidden="true" />
                          {t("locations.insideGame")}
                        </span>
                      ) : path.folder !== null ? (
                        <Button
                          variant="outline"
                          size="sm"
                          className="shrink-0 bg-surface-container dark:bg-surface-high"
                          onClick={() => {
                            openFolder(path.folder ?? "");
                          }}
                        >
                          <FolderOpen />
                          {t("locations.openFolder")}
                        </Button>
                      ) : (
                        <span className="shrink-0 text-xs text-muted-foreground">
                          {t("locations.notFound")}
                        </span>
                      )}
                    </div>
                  );
                })}
              </section>
            ))
          )}
        </div>

        <div className="flex items-center gap-2.5 border-t border-border bg-chrome px-6 py-3">
          <span className="flex size-[30px] shrink-0 items-center justify-center rounded-md bg-surface-container text-foreground-variant dark:bg-surface-high">
            <Bug className="size-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <b className="font-semibold">{t("locations.tellUs")}</b>{" "}
            <span className="text-xs text-muted-foreground">{t("locations.tellUsBody")}</span>
          </div>
          <Button
            variant="secondary"
            size="sm"
            className="ml-auto shrink-0 bg-surface-container dark:bg-surface-high"
            onClick={tellUs}
          >
            <ExternalLink />
            {t("locations.tellUsButton")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function FilterChip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        "inline-flex h-[26px] items-center rounded-full border px-2.5 text-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/60",
        active
          ? "border-primary bg-primary font-semibold text-primary-foreground"
          : "border-border bg-card text-foreground-variant hover:bg-surface-container",
      )}
    >
      {children}
    </button>
  );
}

function Highlight({ text, query }: { text: string; query: string }) {
  return (
    <>
      {highlightText(text, query).map((part, index) =>
        part.hit ? (
          <mark key={index} className="rounded-sm bg-primary/25 px-px text-inherit">
            {part.text}
          </mark>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  );
}

const TOKEN_CLASS = {
  text: "text-foreground",
  env: "text-muted-foreground",
  placeholder: "text-tertiary",
} as const;

function PathText({ text, query }: { text: string; query: string }) {
  return (
    <>
      {tokenizePath(text).map((token, tokenIndex) => (
        <span key={tokenIndex} className={TOKEN_CLASS[token.kind]}>
          {highlightText(token.text, query).map((part, index) =>
            part.hit ? (
              <mark key={index} className="rounded-sm bg-primary/25 px-px text-inherit">
                {part.text}
              </mark>
            ) : (
              <span key={index}>{part.text}</span>
            ),
          )}
        </span>
      ))}
    </>
  );
}
