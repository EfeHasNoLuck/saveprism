import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  Bot,
  Bug,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Download,
  ExternalLink,
  FileText,
  FolderOpen,
  GitBranch as Github,
  Gift,
  History,
  Image as ImageIcon,
  Info,
  Keyboard,
  MessageSquare,
  Palette,
  Pipette,
  RotateCcw,
  Scale,
  Search,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Users,
  Upload,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import licenseText from "../../../../LICENSE?raw";
import {
  backgroundImage,
  chooseBackground,
  defaultPaths,
  exportThemeFile,
  importThemeFile,
  mcpCommand,
  openFolder,
  setAppFolder,
  setBackupFolder,
  type BackgroundImage as BackgroundImageData,
  type DefaultPaths,
} from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Stepper } from "@/components/ui/stepper";
import { Switch } from "@/components/ui/switch";
import { SUPPORTED_LANGUAGES, type Language } from "@/i18n";
import { LanguageFlag } from "@/i18n/flags";
import { languageName } from "@/i18n/language";
import { useTheme, type ThemePreference } from "@/theme";
import { APP_COLOR_PRESETS, appColorSwatch, DEFAULT_APP_COLOR, type AppColor } from "./appColor";
import { storedAccent } from "./appearance";
import { COLOR_PRESETS, DEFAULT_COLORS, isHexColor, textColorFor, tonesFor, type ColorGroup } from "./colors";
import { useNotify, useSettings } from "./context";
import { describeError } from "./errors";
import { Logo } from "./Logo";
import { loadNotices, type NoticePackage } from "./notices";
import {
  clampKeepBackups,
  clampKeepDays,
  type BackgroundMode,
  type PatternKind,
  type PhotoFit,
  type SaveDefault,
} from "./settings";
import { searchSettings, type SettingsTab } from "./settingsSearch";
import { MCP_CLIENTS, mcpSnippet, type McpClient } from "./setup";
import { ShortcutSettings } from "./ShortcutSettings";
import {
  buildThemeFile,
  DEFAULT_THEME_NAME,
  defaultThemeFileName,
  ensureThemeFileName,
  parseThemeFile,
  serializeThemeFile,
  themeSettingsPatch,
} from "./themeFile";
import { WhatsNewDialog } from "./WhatsNewDialog";

const REPO_URL = "https://github.com/huseyincancalti/saveprism";
const COMMUNITY_URL = `${REPO_URL}/discussions`;
const ISSUES_URL = `${REPO_URL}/issues/new`;

const THEMES: ThemePreference[] = ["system", "light", "dark"];

const TABS: { id: SettingsTab; icon: typeof SlidersHorizontal; labelKey: string }[] = [
  { id: "general", icon: SlidersHorizontal, labelKey: "settings.tabs.general" },
  { id: "appearance", icon: Palette, labelKey: "settings.tabs.appearance" },
  { id: "backups", icon: History, labelKey: "settings.tabs.backups" },
  { id: "ai", icon: Sparkles, labelKey: "settings.tabs.ai" },
  { id: "shortcuts", icon: Keyboard, labelKey: "settings.tabs.shortcuts" },
  { id: "about", icon: Info, labelKey: "settings.tabs.about" },
];

type FolderPicker = "backup" | "app";

function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className="inline-flex rounded-sm bg-field p-0.5 ring-1 ring-border"
    >
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className={
            option.value === value
              ? "whitespace-nowrap rounded-[3px] bg-primary/15 px-2.5 py-0.5 text-xs font-semibold text-primary outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              : "whitespace-nowrap rounded-[3px] px-2.5 py-0.5 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
          }
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

function Slider({
  value,
  min,
  max,
  step,
  onChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (value: number) => void;
  label: string;
}) {
  return (
    <input
      type="range"
      min={min}
      max={max}
      step={step}
      value={value}
      onChange={(event) => onChange(Number(event.target.value))}
      aria-label={label}
      className="w-48 min-w-0 shrink accent-primary"
    />
  );
}

function ValuePill({ children }: { children: ReactNode }) {
  return (
    <span className="inline-flex h-7 min-w-13 items-center justify-center rounded-sm border border-input bg-field px-2 font-mono text-xs tabular-nums">
      {children}
    </span>
  );
}

function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mt-5">
      <div className="on-photo mb-1.5 flex h-6 items-center justify-between gap-3">
        <h3 className="text-[0.6875rem] font-semibold tracking-[0.06em] text-muted-foreground uppercase">
          {title}
        </h3>
        {aside}
      </div>
      {children}
    </section>
  );
}

function Group({ children }: { children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-lg border border-border bg-card">
      {children}
    </div>
  );
}

function Row({
  id,
  label,
  description,
  children,
  highlight,
}: {
  id: string;
  label: string;
  description?: string;
  children: ReactNode;
  highlight?: boolean;
}) {
  return (
    <div
      data-setting={id}
      className={`flex min-h-14 flex-wrap items-center gap-x-4 gap-y-2 border-t border-border px-4 py-2.5 first:border-t-0 ${
        highlight ? "bg-primary/8" : ""
      }`}
    >
      <div className="min-w-52 flex-1">
        <b className="block text-[0.84375rem] leading-5 font-medium">{label}</b>
        {description ? (
          <span className="mt-px block text-xs leading-[17px] text-muted-foreground">
            {description}
          </span>
        ) : null}
      </div>
      <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center justify-end gap-2">{children}</div>
    </div>
  );
}

/** The whole Settings page (layout B): a sidebar with the tabs under the app header. */
export function SettingsPage({
  onClose,
  onShowTour,
  photoDropActive,
  photoVersion,
}: {
  onClose: () => void;
  onShowTour: () => void;
  /** A dragged file is over the Photo card's drop zone. */
  photoDropActive: boolean;
  /** Bumped when a dropped photo replaced the file, so the preview reads it again. */
  photoVersion: number;
}) {
  const { t, i18n } = useTranslation();
  const { notify } = useNotify();
  const { settings, update } = useSettings();
  const { preference, setPreference } = useTheme();
  const language: Language = i18n.resolvedLanguage === "tr" ? "tr" : "en";

  const [tab, setTab] = useState<SettingsTab>("general");
  const [query, setQuery] = useState("");
  const [highlight, setHighlight] = useState<string | null>(null);
  const [paths, setPaths] = useState<DefaultPaths | null>(null);
  const [photo, setPhoto] = useState<BackgroundImageData | null>(null);
  const [busy, setBusy] = useState<FolderPicker | "photo" | null>(null);
  const [themeBusy, setThemeBusy] = useState(false);
  const [version, setVersion] = useState("—");
  const [exePath, setExePath] = useState<string | null>(null);
  const [copiedClient, setCopiedClient] = useState<McpClient | null>(null);
  const [noticesPage, setNoticesPage] = useState(false);
  const [noticeQuery, setNoticeQuery] = useState("");
  const [noticePackages, setNoticePackages] = useState<NoticePackage[] | null>(null);
  const [noticeFailed, setNoticeFailed] = useState(false);
  const [expandedNotices, setExpandedNotices] = useState<ReadonlySet<string>>(new Set());
  const [licenseOpen, setLicenseOpen] = useState(false);
  const [whatsNewOpen, setWhatsNewOpen] = useState(false);
  const pageRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    defaultPaths()
      .then((value) => !cancelled && setPaths(value))
      .catch(() => undefined);
    getVersion()
      .then((value) => !cancelled && setVersion(value))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  // Only the About tab needs the notices, so the chunk is not part of the startup bundle.
  useEffect(() => {
    if (tab !== "about") return;
    let cancelled = false;
    loadNotices()
      .then((value) => !cancelled && setNoticePackages(value))
      .catch(() => !cancelled && setNoticeFailed(true));
    return () => {
      cancelled = true;
    };
  }, [tab]);

  // Focus the page once when it opens; focus must stay on a slider after an arrow key.
  useEffect(() => {
    pageRef.current?.focus();
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      // The dialogs close themselves; Settings stays open under them.
      if (licenseOpen || whatsNewOpen) return;
      event.preventDefault();
      if (noticesPage) setNoticesPage(false);
      else onClose();
    };
    window.addEventListener("keydown", listener);
    return () => window.removeEventListener("keydown", listener);
  }, [licenseOpen, noticesPage, onClose, whatsNewOpen]);

  useEffect(() => {
    if (tab !== "ai" || exePath !== null) return;
    let cancelled = false;
    mcpCommand()
      .then((value) => !cancelled && setExePath(value))
      .catch(() => !cancelled && setExePath("saveprism-desktop.exe"));
    return () => {
      cancelled = true;
    };
  }, [tab, exePath]);

  useEffect(() => {
    if (settings.backgroundMode !== "photo") return;
    let cancelled = false;
    backgroundImage(settings.appFolder)
      .then((image) => !cancelled && setPhoto(image))
      .catch(() => !cancelled && setPhoto(null));
    return () => {
      cancelled = true;
    };
  }, [settings.backgroundMode, settings.appFolder, settings.photoName, photoVersion]);

  const results = useMemo(
    () => searchSettings(query, (key) => t(key), language),
    [language, query, t],
  );

  const jumpTo = useCallback((nextTab: SettingsTab, id: string) => {
    setTab(nextTab);
    setQuery("");
    setHighlight(id);
    window.requestAnimationFrame(() => {
      document
        .querySelector(`[data-setting="${id}"]`)
        ?.scrollIntoView({ block: "center", behavior: "smooth" });
    });
  }, []);

  const reportError = useCallback(
    (title: string, error: unknown) => {
      const { reason, fix } = describeError(error, t);
      notify({
        kind: "error",
        title,
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    },
    [notify, t],
  );

  const currentBackup = settings.backupFolders[0] ?? paths?.backupDir ?? "…";
  const appFolder = settings.appFolder ?? paths?.appDir ?? "…";

  const pickFolder = useCallback(
    async (kind: FolderPicker) => {
      if (busy) return;
      setBusy(kind);
      try {
        const picked = await openDialog({
          directory: true,
          multiple: false,
          title: t(
            kind === "backup" ? "settings.backupFolderTitle" : "settings.appFolderTitle",
          ),
        });
        if (typeof picked !== "string") return;
        if (kind === "backup") {
          const roots = await setBackupFolder(picked, settings.backupFolders);
          update({ backupFolders: roots });
        } else {
          const moved = await setAppFolder(picked, settings.appFolder);
          update({ appFolder: moved, photoName: null });
          const image = await backgroundImage(moved);
          setPhoto(image);
        }
        notify({ kind: "saved", title: t("settings.folderChanged") });
      } catch (error) {
        reportError(t("settings.folderFailed"), error);
      } finally {
        setBusy(null);
      }
    },
    [busy, notify, reportError, settings.appFolder, settings.backupFolders, t, update],
  );

  const openChosenFolder = useCallback(
    async (folder: string) => {
      try {
        await openFolder(folder);
      } catch (error) {
        reportError(t("settings.openFolderFailed"), error);
      }
    },
    [reportError, t],
  );

  const pickPhoto = useCallback(async () => {
    if (busy) return;
    setBusy("photo");
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        title: t("settings.chooseImage"),
        filters: [{ name: "PNG, JPEG, WebP", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      if (typeof picked !== "string") return;
      const image = await chooseBackground(picked, settings.appFolder);
      setPhoto(image);
      update({ backgroundMode: "photo", photoName: image.name });
      notify({ kind: "saved", title: t("settings.imageCopied") });
    } catch (error) {
      reportError(t("settings.imageFailed"), error);
    } finally {
      setBusy(null);
    }
  }, [busy, notify, reportError, settings.appFolder, t, update]);

  const exportTheme = useCallback(async () => {
    if (themeBusy) return;
    setThemeBusy(true);
    try {
      const theme = buildThemeFile(settings, DEFAULT_THEME_NAME);
      const picked = await saveDialog({
        defaultPath: defaultThemeFileName(theme.name),
        filters: [{ name: t("settings.themeFile.filterName"), extensions: ["json"] }],
        title: t("settings.themeFile.exportTitle"),
      });
      if (typeof picked !== "string") return;
      await exportThemeFile(ensureThemeFileName(picked), serializeThemeFile(theme));
      notify({ kind: "saved", title: t("settings.themeFile.exported") });
    } catch (error) {
      reportError(t("settings.themeFile.exportFailed"), error);
    } finally {
      setThemeBusy(false);
    }
  }, [notify, reportError, settings, t, themeBusy]);

  const importTheme = useCallback(async () => {
    if (themeBusy) return;
    setThemeBusy(true);
    try {
      const picked = await openDialog({
        multiple: false,
        directory: false,
        title: t("settings.themeFile.importTitle"),
        filters: [{ name: t("settings.themeFile.filterName"), extensions: ["json"] }],
      });
      if (typeof picked !== "string") return;
      const text = await importThemeFile(picked);
      const result = parseThemeFile(text);
      if (!result.ok) {
        notify({
          kind: "error",
          title: t("settings.themeFile.invalid"),
          detail: t(`settings.themeFile.reasons.${result.reason}`),
        });
        return;
      }
      update(themeSettingsPatch(result.theme, settings));
      notify({
        kind: "saved",
        title: t("settings.themeFile.imported", { name: result.theme.name }),
      });
    } catch (error) {
      reportError(t("settings.themeFile.importFailed"), error);
    } finally {
      setThemeBusy(false);
    }
  }, [notify, reportError, settings, t, themeBusy, update]);

  const mcpClient = settings.mcpClient;
  const snippet = mcpSnippet(mcpClient, exePath ?? "saveprism-desktop.exe");

  const copySetup = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(snippet);
      setCopiedClient(mcpClient);
      notify({ kind: "saved", title: t("toast.setupCopied") });
    } catch {
      notify({ kind: "error", title: t("toast.copySetupFailed") });
    }
  }, [mcpClient, notify, snippet, t]);

  const openExternal = useCallback(async (url: string) => {
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener,noreferrer");
    }
  }, []);

  const titleKey = TABS.find((item) => item.id === tab)?.labelKey ?? "settings.tabs.general";
  const notices = noticePackages ?? [];
  const noticeSearch = noticeQuery.trim().toLocaleLowerCase(language);
  const visibleNotices =
    noticeSearch === ""
      ? notices
      : notices.filter((item) =>
          `${item.name} ${item.version} ${item.license}`
            .toLocaleLowerCase(language)
            .includes(noticeSearch),
        );

  return (
    <div
      ref={pageRef}
      tabIndex={-1}
      className="relative z-10 flex min-h-0 flex-1 outline-none"
      aria-label={t("settings.title")}
    >
      <aside className="flex w-63 shrink-0 flex-col border-r border-border bg-chrome px-3 py-3.5">
        <button
          type="button"
          onClick={onClose}
          className="mb-3 inline-flex h-7 w-fit items-center gap-1.5 rounded-sm px-2 text-[0.78rem] font-medium text-foreground-variant outline-none hover:bg-surface-high focus-visible:ring-2 focus-visible:ring-ring/60"
        >
          <ArrowLeft className="size-3.5" />
          {t("settings.back")}
          <kbd className="ml-1 rounded-sm border border-border bg-surface-container px-1 font-mono text-[0.6875rem] text-foreground-variant">
            Esc
          </kbd>
        </button>
        <h1 className="mb-3 px-1 text-xl font-semibold tracking-tight">{t("settings.title")}</h1>

        <div className="relative mb-3">
          <div className="flex h-8 items-center gap-2 rounded-sm border border-input bg-field px-2.5">
            <Search className="size-3.5 shrink-0 text-muted-foreground" />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && results[0]) {
                  event.preventDefault();
                  jumpTo(results[0].tab, results[0].id);
                }
              }}
              placeholder={t("settings.searchPlaceholder")}
              aria-label={t("settings.searchPlaceholder")}
              className="min-w-0 flex-1 bg-transparent text-[0.8125rem] outline-none placeholder:text-muted-foreground"
            />
          </div>
          {query.trim() !== "" ? (
            <div className="absolute inset-x-0 top-9 z-20 overflow-hidden rounded-md border border-border bg-popover shadow-lg">
              {results.length === 0 ? (
                <p className="px-3 py-2.5 text-xs text-muted-foreground">
                  {t("settings.searchEmpty")}
                </p>
              ) : (
                <ul className="py-1">
                  {results.map((result) => (
                    <li key={result.id}>
                      <button
                        type="button"
                        onClick={() => jumpTo(result.tab, result.id)}
                        className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-xs outline-none hover:bg-surface-high focus-visible:bg-surface-high"
                      >
                        <span className="truncate">{t(result.labelKey)}</span>
                        <span className="shrink-0 text-[0.6875rem] text-muted-foreground">
                          {t(`settings.tabs.${result.tab}`)}
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ) : null}
        </div>

        <nav aria-label={t("settings.title")} className="flex flex-col gap-0.5">
          {TABS.map(({ id, icon: Icon, labelKey }) => (
            <button
              key={id}
              type="button"
              aria-current={tab === id ? "page" : undefined}
              onClick={() => {
                setTab(id);
                setNoticesPage(false);
                setHighlight(null);
              }}
              className={
                tab === id
                  ? "flex h-9 items-center gap-2.5 rounded-sm bg-primary/14 px-2.5 text-left text-[0.84375rem] font-semibold text-primary outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                  : "flex h-9 items-center gap-2.5 rounded-sm px-2.5 text-left text-[0.84375rem] text-foreground-variant outline-none hover:bg-surface-high hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
              }
            >
              <Icon className="size-4 shrink-0" />
              {t(labelKey)}
            </button>
          ))}
        </nav>

        <div className="mt-auto flex flex-col gap-1.5 border-t border-border px-1.5 pt-2.5 text-[0.78rem]">
          <button
            type="button"
            onClick={() => void openExternal(COMMUNITY_URL)}
            className="flex items-center gap-2 rounded-sm text-left text-foreground-variant outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <Users className="size-4 text-muted-foreground" />
            {t("settings.communityThemes")}
          </button>
          <span className="mt-1 text-[0.71875rem] text-muted-foreground">
            {t("settings.version", { version })}
          </span>
        </div>
      </aside>

      <section className="dense-scroll min-w-0 flex-1 overflow-y-auto px-10 pt-6 pb-8">
        <div className="mx-auto max-w-[948px]">
          {noticesPage ? (
            <div>
              <div className="on-photo mb-1 flex items-center gap-1 text-xs text-muted-foreground">
                <button
                  type="button"
                  onClick={() => setNoticesPage(false)}
                  className="inline-flex items-center gap-1 rounded-sm font-medium text-primary outline-none hover:text-primary/80 focus-visible:ring-2 focus-visible:ring-ring/60"
                >
                  <ArrowLeft className="size-3.5" />
                  {t("settings.tabs.about")}
                </button>
                <ChevronRight className="size-3" />
                <span>{t("about.notices")}</span>
              </div>
              <h2 className="on-photo text-[1.375rem] leading-7 font-semibold tracking-tight">
                {t("about.notices")}
              </h2>
              <p className="on-photo mt-0.5 text-xs text-muted-foreground">{t("about.noticesSubtitle")}</p>

              <div className="mt-4 flex items-center gap-3">
                <div className="flex h-8 max-w-xs flex-1 items-center gap-2 rounded-sm border border-input bg-field px-2.5">
                  <Search className="size-3.5 shrink-0 text-muted-foreground" />
                  <input
                    value={noticeQuery}
                    onChange={(event) => setNoticeQuery(event.target.value)}
                    placeholder={t("about.searchPackages")}
                    aria-label={t("about.searchPackages")}
                    className="min-w-0 flex-1 bg-transparent text-[0.8125rem] outline-none placeholder:text-muted-foreground"
                  />
                </div>
                <span className="text-xs text-muted-foreground">
                  {noticeFailed
                    ? t("about.noticesFailed")
                    : noticePackages === null
                      ? t("common.loading")
                      : noticeSearch === ""
                        ? t("about.noticesCount", { total: notices.length })
                        : t("about.noticesCountFiltered", {
                            shown: visibleNotices.length,
                            total: notices.length,
                          })}
                </span>
              </div>

              <div className="mt-3 overflow-hidden rounded-lg border border-border bg-card">
                {noticeFailed ? (
                  <p className="px-4 py-3 text-xs text-muted-foreground">
                    {t("about.noticesFailed")}
                  </p>
                ) : noticePackages === null ? (
                  <p className="px-4 py-3 text-xs text-muted-foreground">{t("common.loading")}</p>
                ) : visibleNotices.length === 0 ? (
                  <p className="px-4 py-3 text-xs text-muted-foreground">
                    {t("about.searchEmpty")}
                  </p>
                ) : (
                  visibleNotices.map((item, index) => {
                    // Same name/version/license can appear with different license texts; the
                    // index keeps React keys and the expanded rows unique.
                    const id = `${item.name}@${item.version}@${item.license}@${index}`;
                    const open = expandedNotices.has(id);
                    return (
                      <div
                        key={id}
                        className={index > 0 ? "border-t border-border" : undefined}
                      >
                        <button
                          type="button"
                          aria-expanded={open}
                          onClick={() =>
                            setExpandedNotices((current) => {
                              const next = new Set(current);
                              if (next.has(id)) next.delete(id);
                              else next.add(id);
                              return next;
                            })
                          }
                          className="flex h-10 w-full items-center gap-2.5 px-3.5 text-left text-[0.8125rem] outline-none hover:bg-surface-high focus-visible:bg-surface-high"
                        >
                          {open ? (
                            <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
                          ) : (
                            <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
                          )}
                          <span className="truncate font-medium">{item.name}</span>
                          <span className="font-mono text-[0.6875rem] text-muted-foreground">
                            {item.version}
                          </span>
                          <span className="ml-auto shrink-0 rounded-sm bg-surface-high px-1.5 py-0.5 font-mono text-[0.6875rem] font-semibold text-tertiary">
                            {item.license || t("about.licenseUnknown")}
                          </span>
                        </button>
                        {open ? (
                          <pre className="dense-scroll mx-3.5 mb-3 max-h-72 overflow-auto rounded-sm border border-border bg-field p-3 font-mono text-[0.6875rem] leading-5 break-words whitespace-pre-wrap text-foreground-variant dark:border-surface-highest">
                            {item.text || t("about.noticeNoText")}
                          </pre>
                        ) : null}
                      </div>
                    );
                  })
                )}
              </div>
            </div>
          ) : (
            <>
          <div className="on-photo mb-1 flex items-center gap-1 text-xs text-muted-foreground">
            {t("settings.title")}
            <ChevronRight className="size-3" />
            <span>{t(titleKey)}</span>
          </div>
          <h2 className="on-photo text-[1.375rem] leading-7 font-semibold tracking-tight">
            {t(titleKey)}
          </h2>
          {tab === "about" ? null : (
            <p className="on-photo mt-0.5 text-xs text-muted-foreground">{t(`settings.sub.${tab}`)}</p>
          )}

          {tab === "general" ? (
            <div className="mt-4">
              <Section title={t("settings.tabs.general")}>
                <Group>
                  <Row
                    id="general.language"
                    label={t("settings.language")}
                    highlight={highlight === "general.language"}
                  >
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="outline" size="sm" className="min-w-36 justify-between">
                          <span className="flex items-center gap-2">
                            <LanguageFlag language={language} className="size-4" />
                            {languageName(language)}
                          </span>
                          <ChevronDown className="size-3.5 text-muted-foreground" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end" className="min-w-44">
                        <DropdownMenuLabel>{t("language.label")}</DropdownMenuLabel>
                        <DropdownMenuSeparator />
                        <DropdownMenuRadioGroup
                          value={language}
                          onValueChange={(value) => {
                            if (value === "en" || value === "tr") {
                              void i18n.changeLanguage(value);
                            }
                          }}
                        >
                          {SUPPORTED_LANGUAGES.map((lang) => (
                            <DropdownMenuRadioItem key={lang} value={lang} className="gap-2">
                              <LanguageFlag language={lang} className="size-4" />
                              {languageName(lang)}
                            </DropdownMenuRadioItem>
                          ))}
                        </DropdownMenuRadioGroup>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </Row>
                  <Row
                    id="general.theme"
                    label={t("settings.theme")}
                    highlight={highlight === "general.theme"}
                  >
                    <Segmented
                      label={t("settings.theme")}
                      value={preference}
                      options={THEMES.map((value) => ({
                        value,
                        label: t(`theme.${value}`),
                      }))}
                      onChange={setPreference}
                    />
                  </Row>
                  <Row
                    id="general.notifications"
                    label={t("settings.showNotifications")}
                    description={t("settings.showNotificationsInfo")}
                    highlight={highlight === "general.notifications"}
                  >
                    <Switch
                      checked={settings.showNotifications}
                      onCheckedChange={(checked) => update({ showNotifications: checked })}
                      aria-label={t("settings.showNotifications")}
                    />
                  </Row>
                </Group>
              </Section>
            </div>
          ) : null}

          {tab === "appearance" ? (
            <div className="grid grid-cols-[minmax(0,1fr)_280px] gap-6">
              <div>
                <Section
                  title={t("settings.appColor")}
                  aside={
                    <Button
                      variant="ghost"
                      size="xs"
                      className="text-foreground-variant"
                      onClick={() => update({ appColor: { ...DEFAULT_APP_COLOR } })}
                    >
                      <RotateCcw className="size-3" />
                      {t("settings.resetColors")}
                    </Button>
                  }
                >
                  <Group>
                    <AppColorRow
                      id="appearance.appColor"
                      value={settings.appColor}
                      onChange={(appColor) => update({ appColor })}
                      highlight={highlight === "appearance.appColor"}
                    />
                  </Group>
                </Section>

                <Section title={t("settings.text")}>
                  <Group>
                    <Row
                      id="appearance.textSize"
                      label={t("settings.textSize")}
                      description={t("settings.textSizeHint")}
                      highlight={highlight === "appearance.textSize"}
                    >
                      <span aria-hidden className="text-[0.6875rem] font-semibold text-muted-foreground">A</span>
                      <Slider
                        value={settings.textScale}
                        min={80}
                        max={140}
                        step={5}
                        label={t("settings.textSize")}
                        onChange={(textScale) => update({ textScale })}
                      />
                      <span aria-hidden className="text-[1.0625rem] font-semibold text-muted-foreground">A</span>
                      <ValuePill>{settings.textScale}%</ValuePill>
                    </Row>
                  </Group>
                </Section>

                <Section
                  title={t("settings.colors")}
                  aside={
                    <Button
                      variant="ghost"
                      size="xs"
                      className="text-foreground-variant"
                      onClick={() => update({ colors: DEFAULT_COLORS })}
                    >
                      <RotateCcw className="size-3" />
                      {t("settings.resetColors")}
                    </Button>
                  }
                >
                  <Group>
                    <ColorRow
                      group="changed"
                      id="appearance.changed"
                      title={t("settings.changedColor")}
                      description={t("settings.changedHint")}
                      value={settings.colors.changed}
                      onChange={(changed) => update({ colors: { ...settings.colors, changed } })}
                      highlight={highlight === "appearance.changed"}
                    />
                    <ColorRow
                      group="accent"
                      id="appearance.accent"
                      title={t("settings.accentColor")}
                      description={t("settings.accentHint")}
                      value={settings.colors.accent}
                      onChange={(accent) => update({ colors: { ...settings.colors, accent } })}
                      highlight={highlight === "appearance.accent"}
                    />
                    <ColorRow
                      group="error"
                      id="appearance.error"
                      title={t("settings.errorColor")}
                      description={t("settings.errorHint")}
                      value={settings.colors.error}
                      onChange={(error) => update({ colors: { ...settings.colors, error } })}
                      highlight={highlight === "appearance.error"}
                    />
                  </Group>
                </Section>

                <Section title={t("settings.background")}>
                  <div
                    data-setting="appearance.background"
                    className={`grid grid-cols-[repeat(auto-fit,minmax(min(100%,11rem),1fr))] gap-3 ${highlight === "appearance.background" ? "rounded-lg ring-2 ring-primary/40" : ""}`}
                  >
                    <BackgroundCard
                      id="appearance.pattern"
                      mode="pattern"
                      current={settings.backgroundMode}
                      title={t("settings.pattern")}
                      onSelect={() => update({ backgroundMode: "pattern" })}
                    >
                      <PatternTiles
                        value={settings.patternKind}
                        onChange={(patternKind) => update({ patternKind })}
                      />
                      <MiniSlider
                        id="appearance.strength"
                        label={t("settings.strength")}
                        value={settings.patternStrength}
                        onChange={(patternStrength) => update({ patternStrength })}
                      />
                    </BackgroundCard>

                    <BackgroundCard
                      id="appearance.photo"
                      mode="photo"
                      current={settings.backgroundMode}
                      title={t("settings.photo")}
                      onSelect={() => update({ backgroundMode: "photo" })}
                      highlight={photoDropActive}
                    >
                      <button
                        type="button"
                        data-photo-drop
                        onClick={() => void pickPhoto()}
                        disabled={busy === "photo"}
                        aria-label={photo ? t("settings.changeImage") : t("settings.chooseImage")}
                        className={`group relative flex h-24 w-full items-center justify-center overflow-hidden rounded-md border-2 border-dashed text-muted-foreground outline-none transition-colors duration-150 ease-out focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60 motion-reduce:transition-none ${
                          photoDropActive
                            ? "border-primary bg-primary/12 text-primary"
                            : "border-input hover:border-primary/60 hover:bg-surface-high/40"
                        }`}
                      >
                        {photo ? (
                          <>
                            <img
                              src={photo.dataUrl}
                              alt=""
                              className="absolute inset-0 size-full object-cover"
                            />
                            <span className="absolute inset-x-0 bottom-0 flex items-center justify-center bg-black/60 py-1 text-[0.6875rem] font-medium text-white opacity-0 transition-opacity duration-150 ease-out group-hover:opacity-100 group-focus-visible:opacity-100 motion-reduce:transition-none">
                              {t("settings.changeImage")}
                            </span>
                          </>
                        ) : (
                          <span className="flex flex-col items-center gap-1">
                            <ImageIcon className="size-5" />
                            <span
                              className={`text-[0.6875rem] ${photoDropActive ? "font-semibold" : ""}`}
                            >
                              {t(photoDropActive ? "settings.dropActive" : "settings.dropHint")}
                            </span>
                          </span>
                        )}
                        {photo && photoDropActive ? (
                          <span className="absolute inset-0 flex items-center justify-center bg-black/60 text-[0.6875rem] font-semibold text-white">
                            {t("settings.dropActive")}
                          </span>
                        ) : null}
                      </button>
                      <div className="mt-2 min-w-0 text-xs text-muted-foreground">
                        <b className="block truncate text-[0.78rem] font-medium text-foreground">
                          {photo?.name ?? t("settings.noImage")}
                        </b>
                        {t("settings.imageCopiedHint")}
                      </div>
                      <div
                        data-setting="appearance.fit"
                        className="mt-2.5 flex flex-wrap items-center justify-between gap-2"
                      >
                        <Button
                          variant="secondary"
                          size="xs"
                          disabled={busy === "photo"}
                          onClick={() => void pickPhoto()}
                        >
                          <ImageIcon className="size-3" />
                          {photo ? t("settings.changeImage") : t("settings.chooseImage")}
                        </Button>
                        <Segmented<PhotoFit>
                          label={t("settings.photoFit")}
                          value={settings.photoFit}
                          options={[
                            { value: "fit", label: t("settings.fit") },
                            { value: "fill", label: t("settings.fill") },
                          ]}
                          onChange={(photoFit) => update({ photoFit })}
                        />
                      </div>
                      <MiniSlider
                        id="appearance.blur"
                        label={t("settings.blur")}
                        value={settings.photoBlur}
                        onChange={(photoBlur) => update({ photoBlur })}
                      />
                      <MiniSlider
                        id="appearance.darken"
                        label={t("settings.darken")}
                        value={settings.photoDarken}
                        onChange={(photoDarken) => update({ photoDarken })}
                      />
                    </BackgroundCard>
                  </div>
                </Section>

                <Section
                  title={t("settings.themeFile.title")}
                  aside={
                    <Button
                      variant="ghost"
                      size="xs"
                      className="text-foreground-variant"
                      onClick={() => void openExternal(COMMUNITY_URL)}
                    >
                      <ExternalLink className="size-3" />
                      {t("settings.themeFile.share")}
                    </Button>
                  }
                >
                  <Group>
                    <Row
                      id="appearance.themeFile"
                      label={t("settings.themeFile.label")}
                      description={t("settings.themeFile.hint")}
                      highlight={highlight === "appearance.themeFile"}
                    >
                      <Button
                        variant="secondary"
                        size="xs"
                        disabled={themeBusy}
                        onClick={() => void exportTheme()}
                      >
                        <Download className="size-3" />
                        {t("settings.themeFile.export")}
                      </Button>
                      <Button
                        variant="secondary"
                        size="xs"
                        disabled={themeBusy}
                        onClick={() => void importTheme()}
                      >
                        <Upload className="size-3" />
                        {t("settings.themeFile.import")}
                      </Button>
                    </Row>
                  </Group>
                </Section>
              </div>

              <PreviewCard photo={photo} />
            </div>
          ) : null}

          {tab === "backups" ? (
            <div>
              <Section title={t("settings.saving")}>
                <Group>
                  <Row
                    id="backups.action"
                    label={t("settings.defaultAction")}
                    description={t("settings.defaultActionInfo")}
                    highlight={highlight === "backups.action"}
                  >
                    <Segmented<SaveDefault>
                      label={t("settings.defaultAction")}
                      value={settings.saveDefault}
                      options={[
                        { value: "saveAs", label: t("settings.saveAsNew") },
                        { value: "overwrite", label: t("settings.overwriteOriginal") },
                      ]}
                      onChange={(saveDefault) => update({ saveDefault })}
                    />
                  </Row>
                </Group>
              </Section>

              <Section title={t("settings.backupsSection")}>
                <Group>
                  <Row
                    id="backups.keep"
                    label={t("settings.keepBackups")}
                    description={t("settings.keepBackupsHint")}
                    highlight={highlight === "backups.keep"}
                  >
                    <div className="flex flex-col gap-1">
                      <Slider
                        value={settings.keepBackups}
                        min={1}
                        max={50}
                        step={1}
                        label={t("settings.keepBackups")}
                        onChange={(keepBackups) => update({ keepBackups: clampKeepBackups(keepBackups) })}
                      />
                      <div className="flex w-48 justify-between text-[0.6875rem] text-muted-foreground">
                        <span>{t("settings.onlyLatest")}</span>
                        <span>50</span>
                      </div>
                    </div>
                    <ValuePill>{settings.keepBackups}</ValuePill>
                  </Row>
                  <Row
                    id="backups.days"
                    label={t("settings.keepDays")}
                    description={t("settings.keepDaysHint")}
                    highlight={highlight === "backups.days"}
                  >
                    <input
                      type="number"
                      min={1}
                      max={365}
                      value={settings.keepBackupDays}
                      onChange={(event) =>
                        update({
                          keepBackupDays: clampKeepDays(Number(event.target.value)),
                        })
                      }
                      aria-label={t("settings.keepDays")}
                      className="h-7 w-16 rounded-sm border border-input bg-field px-1.5 text-right font-mono text-xs tabular-nums outline-none focus-visible:border-ring"
                    />
                    <span className="text-xs text-muted-foreground">{t("settings.days")}</span>
                  </Row>
                  <div className="flex items-start gap-2.5 border-t border-border bg-primary/7 px-4 py-2.5 text-xs text-foreground">
                    <ShieldCheck className="mt-px size-4 shrink-0 text-primary" />
                    {t("settings.latestKeptNote")}
                  </div>
                  <Row
                    id="backups.folder"
                    label={t("settings.backupFolder")}
                    description={currentBackup}
                    highlight={highlight === "backups.folder"}
                  >
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void openChosenFolder(currentBackup)}
                    >
                      <FolderOpen />
                      {t("settings.openFolder")}
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={busy === "backup"}
                      onClick={() => void pickFolder("backup")}
                    >
                      {t("settings.change")}
                    </Button>
                  </Row>
                </Group>
              </Section>

              <Section title={t("settings.appFolderSection")}>
                <Group>
                  <Row
                    id="backups.appFolder"
                    label={t("settings.appFolder")}
                    description={t("settings.appFolderHint")}
                    highlight={highlight === "backups.appFolder"}
                  >
                    <span className="max-w-64 truncate font-mono text-[0.71875rem] text-foreground-variant" title={appFolder}>
                      {appFolder}
                    </span>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => void openChosenFolder(appFolder)}
                    >
                      <FolderOpen />
                      {t("settings.openFolder")}
                    </Button>
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={busy === "app"}
                      onClick={() => void pickFolder("app")}
                    >
                      {t("settings.change")}
                    </Button>
                  </Row>
                </Group>
              </Section>
            </div>
          ) : null}

          {tab === "ai" ? (
            <div className="mt-4 flex flex-col gap-3">
              <div className="rounded-lg border border-border bg-card px-4 py-3.5">
                <div className="flex items-start gap-3">
                  <span className="flex size-[34px] shrink-0 items-center justify-center rounded-lg bg-surface-high text-primary">
                    <MessageSquare className="size-[18px]" />
                  </span>
                  <div>
                    <b className="block text-sm leading-[19px] font-semibold">
                      {t("settings.aiChatTitle")}
                      <span className="ml-1.5 rounded-full bg-surface-high px-1.5 py-px align-[1px] text-[11px] font-semibold text-foreground-variant">
                        {t("settings.aiChatTag")}
                      </span>
                    </b>
                    <p className="mt-px text-xs leading-[17px] text-muted-foreground">
                      {t("settings.aiChatInfo")}
                    </p>
                  </div>
                </div>
                <Stepper
                  current={-1}
                  label={t("settings.aiChatTitle")}
                  steps={[
                    {
                      title: t("settings.aiChatStep1"),
                      description: t("settings.aiChatStep1Info"),
                    },
                    {
                      title: t("settings.aiChatStep2"),
                      description: t("settings.aiChatStep2Info"),
                    },
                    {
                      title: t("settings.aiChatStep3"),
                      description: t("settings.aiChatStep3Info"),
                    },
                  ]}
                />
              </div>
              <div className="rounded-lg border border-border bg-card px-4 py-3.5">
                <div className="flex items-start gap-3">
                  <span className="flex size-[34px] shrink-0 items-center justify-center rounded-lg bg-surface-high text-primary">
                    <Bot className="size-[18px]" />
                  </span>
                  <div>
                    <b className="block text-sm leading-[19px] font-semibold">
                      {t("settings.mcpTitle")}
                    </b>
                    <p className="mt-px text-xs leading-[17px] text-muted-foreground">
                      {t("settings.mcpHint")}
                    </p>
                  </div>
                </div>
                <Stepper
                  current={copiedClient === mcpClient ? 2 : 1}
                  done={[true, copiedClient === mcpClient, false, false]}
                  label={t("settings.mcpTitle")}
                  steps={[
                    {
                      title: t("settings.mcpStep1"),
                      description: t(`settings.mcpClients.${mcpClient}`),
                    },
                    {
                      title: t("settings.mcpStep2"),
                      description: t("settings.mcpStep2Info"),
                    },
                    {
                      title: t("settings.mcpStep3"),
                      description: t("settings.mcpStep3Info"),
                    },
                    {
                      title: t("settings.mcpStep4"),
                      description: t("settings.mcpStep4Info"),
                    },
                  ]}
                />
                <div className="mt-3 border-t border-border pt-3">
                  <div
                    role="radiogroup"
                    aria-label={t("settings.mcpYourApp")}
                    className="mb-2.5 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground"
                  >
                    <span className="mr-0.5">{t("settings.mcpYourApp")}</span>
                    {MCP_CLIENTS.map((client) => {
                      const selected = client === mcpClient;
                      return (
                        <button
                          key={client}
                          type="button"
                          role="radio"
                          aria-checked={selected}
                          onClick={() => {
                            update({ mcpClient: client });
                            setCopiedClient(null);
                          }}
                          className={
                            selected
                              ? "inline-flex h-[26px] items-center gap-1.5 rounded-full border border-primary bg-primary/12 px-2.5 text-xs font-semibold text-primary outline-none transition-colors duration-150 ease-out focus-visible:ring-2 focus-visible:ring-ring/60 motion-reduce:transition-none"
                              : "inline-flex h-[26px] items-center gap-1.5 rounded-full border border-border bg-card px-2.5 text-xs text-foreground-variant outline-none transition-colors duration-150 ease-out hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 motion-reduce:transition-none dark:border-surface-highest"
                          }
                        >
                          {selected ? <Check className="size-3" strokeWidth={3} /> : null}
                          {t(`settings.mcpClients.${client}`)}
                        </button>
                      );
                    })}
                  </div>
                  <div className="relative">
                    <pre className="overflow-x-auto rounded-md border border-border bg-field py-2.5 pr-32 pl-3 font-mono text-[11.5px] leading-4 whitespace-pre text-foreground-variant dark:border-surface-highest">
                      {snippet}
                    </pre>
                    <Button
                      size="sm"
                      className="absolute top-2 right-2"
                      onClick={() => void copySetup()}
                    >
                      <Copy />
                      {t("settings.copySetup")}
                    </Button>
                  </div>
                  <p className="mt-2.5 flex items-center gap-2 text-xs text-foreground-variant">
                    <Info className="size-3.5 shrink-0" />
                    <span>{t(`settings.mcpHowTo.${mcpClient}`)}</span>
                  </p>
                </div>
              </div>
              <p className="text-xs leading-[17px] text-muted-foreground">
                {t("settings.aiPrivacyNote")} {t("settings.aiWriteNote")}
              </p>
            </div>
          ) : null}

          {tab === "shortcuts" ? <ShortcutSettings /> : null}

          {tab === "about" ? (
            <div className="mt-5 flex flex-col items-center">
              <span className="flex size-[84px] items-center justify-center rounded-[20px] bg-[linear-gradient(160deg,#13294a,#061428)] shadow-[0_10px_30px_-10px_rgba(0,0,0,0.6),inset_0_0_0_1px_rgba(171,199,255,0.14)]">
                <Logo className="size-14" />
              </span>
              <h3 className="mt-3.5 text-[22px] font-semibold tracking-[-0.02em]">SavePrism</h3>
              <p className="mt-1 text-xs text-muted-foreground">
                {t("about.versionLine", { version })}
              </p>
              <p className="mt-2.5 max-w-md text-center text-[0.8125rem] text-foreground-variant">
                {t("about.tagline")}
              </p>
              <div className="mt-4 flex w-[440px] max-w-full flex-col gap-3">
                <Group>
                  <AboutAction
                    icon={<Gift className="size-3.5" />}
                    label={t("about.whatsNew", { version })}
                    onClick={() => setWhatsNewOpen(true)}
                  />
                  <AboutAction
                    icon={<FileText className="size-3.5" />}
                    label={t("about.notices")}
                    value={
                      noticeFailed
                        ? undefined
                        : noticePackages === null
                          ? t("common.loading")
                          : t("about.noticesCount", { total: noticePackages.length })
                    }
                    onClick={() => setNoticesPage(true)}
                  />
                  <AboutAction
                    icon={<Scale className="size-3.5" />}
                    label={t("about.license")}
                    value={t("about.licenseShort")}
                    onClick={() => setLicenseOpen(true)}
                  />
                  <AboutAction
                    icon={<RotateCcw className="size-3.5" />}
                    label={t("about.showTour")}
                    onClick={onShowTour}
                  />
                </Group>
                <Group>
                  <AboutExternal
                    icon={<Github className="size-3.5" />}
                    label={t("settings.sourceCode")}
                    onClick={() => void openExternal(REPO_URL)}
                  />
                  <AboutExternal
                    icon={<Users className="size-3.5" />}
                    label={t("settings.communityThemes")}
                    onClick={() => void openExternal(COMMUNITY_URL)}
                  />
                  <AboutExternal
                    icon={<Bug className="size-3.5" />}
                    label={t("settings.reportProblem")}
                    onClick={() => void openExternal(ISSUES_URL)}
                  />
                </Group>
              </div>
              <p className="mt-3.5 text-[0.71875rem] text-muted-foreground">
                {t("about.copyright")}
              </p>
            </div>
          ) : null}
            </>
          )}
        </div>
      </section>

      {licenseOpen ? (
        <Dialog open onOpenChange={(open) => (open ? undefined : setLicenseOpen(false))}>
          <DialogContent className="max-w-2xl">
            <DialogHeader>
              <DialogTitle>{t("about.license")}</DialogTitle>
              <DialogDescription>{t("about.licenseBody")}</DialogDescription>
            </DialogHeader>
            <pre className="dense-scroll max-h-[60vh] overflow-auto rounded-sm border border-border bg-field p-3 font-mono text-[0.6875rem] leading-5 whitespace-pre-wrap text-foreground-variant dark:border-surface-highest">
              {licenseText}
            </pre>
          </DialogContent>
        </Dialog>
      ) : null}

      {whatsNewOpen ? (
        <WhatsNewDialog version={version} onClose={() => setWhatsNewOpen(false)} />
      ) : null}
    </div>
  );
}

function AboutAction({
  icon,
  label,
  value,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  value?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-11 w-full items-center gap-3 border-t border-border px-3.5 text-left text-[0.84375rem] outline-none first:border-t-0 hover:bg-surface-high focus-visible:bg-surface-high"
    >
      <span className="flex size-7 items-center justify-center rounded-md bg-surface-high text-primary">
        {icon}
      </span>
      <span>{label}</span>
      <span className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground">
        {value}
        <ChevronRight className="size-3.5 shrink-0" />
      </span>
    </button>
  );
}

function AboutExternal({
  icon,
  label,
  onClick,
}: {
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-11 w-full items-center gap-3 border-t border-border px-3.5 text-left text-[0.84375rem] outline-none first:border-t-0 hover:bg-surface-high focus-visible:bg-surface-high"
    >
      <span className="flex size-7 items-center justify-center rounded-md bg-surface-high text-primary">
        {icon}
      </span>
      {label}
      <ExternalLink className="ml-auto size-3.5 text-muted-foreground" />
    </button>
  );
}

function ColorRow({
  group,
  id,
  title,
  description,
  value,
  onChange,
  highlight,
}: {
  group: ColorGroup;
  id: string;
  title: string;
  description: string;
  value: string;
  onChange: (value: string) => void;
  highlight: boolean;
}) {
  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  const custom = isHexColor(value);
  const current = tonesFor(group, value, dark).main;
  const swatch = custom ? value : current;

  return (
    <Row id={id} label={title} description={description} highlight={highlight}>
      <div className="flex items-center gap-2">
        {COLOR_PRESETS[group].map((preset) => {
          const selected = preset.id === value;
          const color = (dark ? preset.dark : preset.light).main;
          const name = t(`settings.colorPresets.${group}.${preset.id}`);
          return (
            <button
              key={preset.id}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={name}
              title={name}
              onClick={() => onChange(preset.id)}
              className="relative size-6 rounded-full p-0.5 outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
              style={{
                boxShadow: selected
                  ? `0 0 0 2px var(--card), 0 0 0 4px ${textColorFor(color)}`
                  : undefined,
              }}
            >
              <span
                aria-hidden
                className="block size-full rounded-full ring-1 ring-black/15"
                style={{ backgroundColor: color }}
              />
              {selected ? (
                <Check
                  className="absolute inset-0 m-auto size-3"
                  style={{ color: textColorFor(color) }}
                  strokeWidth={3.5}
                />
              ) : null}
            </button>
          );
        })}
        <span className="mx-0.5 h-5 w-px bg-border" />
        <label
          className="relative size-6 cursor-pointer rounded-full p-0.5 outline-none focus-within:ring-2 focus-within:ring-ring/60"
          title={t("settings.customColor")}
          style={{
            boxShadow: custom
              ? `0 0 0 2px var(--card), 0 0 0 4px ${textColorFor(swatch)}`
              : undefined,
          }}
        >
          <span
            aria-hidden
            className="flex size-full items-center justify-center rounded-full"
            style={{
              background: custom
                ? swatch
                : "conic-gradient(#ff5f6d, #ffc371, #f9f871, #6fdba8, #4cd7f6, #7b8cff, #d0bcff, #ff5f6d)",
            }}
          >
            {custom ? (
              <Check className="size-3" style={{ color: textColorFor(swatch) }} strokeWidth={3.5} />
            ) : (
              <Pipette className="size-2.5 text-[#061428]/70" />
            )}
          </span>
          <input
            type="color"
            value={custom ? value : current}
            aria-label={t("settings.customColor")}
            onChange={(event) => onChange(event.target.value)}
            className="absolute inset-0 size-full cursor-pointer opacity-0"
          />
        </label>
        {custom ? (
          <span className="inline-flex h-6 items-center gap-1.5 rounded-sm border border-input bg-field px-1.5 font-mono text-[0.6875rem]">
            <i aria-hidden className="size-2.5 rounded-[3px]" style={{ backgroundColor: value }} />
            {value.toUpperCase()}
          </span>
        ) : null}
      </div>
    </Row>
  );
}

function AppColorRow({
  id,
  value,
  onChange,
  highlight,
}: {
  id: string;
  value: AppColor;
  onChange: (value: AppColor) => void;
  highlight: boolean;
}) {
  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  const isSelected = (candidate: AppColor): boolean => {
    if (candidate.kind !== value.kind) return false;
    if (candidate.kind === "preset" && value.kind === "preset") {
      return candidate.id === value.id;
    }
    if (candidate.kind === "custom" && value.kind === "custom") {
      return true;
    }
    return true;
  };
  const custom = value.kind === "custom" ? value.hex : null;
  const customSwatch = appColorSwatch(value, dark);

  const swatch = (
    key: string,
    selected: boolean,
    color: string,
    label: string,
    onSelect: () => void,
    background?: string,
  ) => (
    <button
      key={key}
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={label}
      title={label}
      onClick={onSelect}
      className="relative size-6 rounded-full p-0.5 outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      style={{
        boxShadow: selected
          ? `0 0 0 2px var(--card), 0 0 0 4px ${textColorFor(color)}`
          : undefined,
      }}
    >
      <span
        aria-hidden
        className="block size-full rounded-full ring-1 ring-black/15"
        style={{ background: background ?? color }}
      />
      {selected ? (
        <Check
          className="absolute inset-0 m-auto size-3"
          style={{ color: textColorFor(color) }}
          strokeWidth={3.5}
        />
      ) : null}
    </button>
  );

  return (
    <Row
      id={id}
      label={t("settings.appColor")}
      description={t("settings.appColorHint")}
      highlight={highlight}
    >
      <div
        role="radiogroup"
        aria-label={t("settings.appColor")}
        className="flex flex-wrap items-center gap-2"
      >
        {APP_COLOR_PRESETS.map((preset) => {
          const choice: AppColor = { kind: "preset", id: preset.id };
          const color = appColorSwatch(choice, dark);
          return swatch(
            preset.id,
            isSelected(choice),
            color,
            t(`settings.appColorPresets.${preset.id}`),
            () => onChange(choice),
          );
        })}
        <span className="mx-0.5 h-5 w-px bg-border" />
        {swatch(
          "system",
          isSelected({ kind: "system" }),
          appColorSwatch({ kind: "system" }, dark),
          t("settings.appColorSystem"),
          () => onChange({ kind: "system" }),
          // The accent seen on the last read, else Windows' default blue: unlike the Custom rainbow.
          storedAccent() ?? "#0078d4",
        )}
        {swatch(
          "mono",
          isSelected({ kind: "mono" }),
          appColorSwatch({ kind: "mono" }, dark),
          t("settings.appColorMono"),
          () => onChange({ kind: "mono" }),
          "linear-gradient(135deg, #f4f4f4 0 50%, #3a3a3a 50% 100%)",
        )}
        <label
          className="relative size-6 cursor-pointer rounded-full p-0.5 outline-none focus-within:ring-2 focus-within:ring-ring/60"
          title={t("settings.appColorCustom")}
          style={{
            boxShadow: custom
              ? `0 0 0 2px var(--card), 0 0 0 4px ${textColorFor(customSwatch)}`
              : undefined,
          }}
        >
          <span
            aria-hidden
            className="flex size-full items-center justify-center rounded-full"
            style={{
              background: custom
                ? custom
                : "conic-gradient(#ff5f6d, #ffc371, #f9f871, #6fdba8, #4cd7f6, #7b8cff, #d0bcff, #ff5f6d)",
            }}
          >
            {custom ? (
              <Check
                className="size-3"
                style={{ color: textColorFor(custom) }}
                strokeWidth={3.5}
              />
            ) : (
              <Pipette className="size-2.5 text-[#061428]/70" />
            )}
          </span>
          <input
            type="color"
            value={custom ?? "#4a7fd6"}
            aria-label={t("settings.appColorCustom")}
            onChange={(event) => onChange({ kind: "custom", hex: event.target.value })}
            className="absolute inset-0 size-full cursor-pointer opacity-0"
          />
        </label>
        {custom ? (
          <span className="inline-flex h-6 items-center gap-1.5 rounded-sm border border-input bg-field px-1.5 font-mono text-[0.6875rem]">
            <i aria-hidden className="size-2.5 rounded-[3px]" style={{ backgroundColor: custom }} />
            {custom.toUpperCase()}
          </span>
        ) : null}
      </div>
    </Row>
  );
}

function BackgroundCard({
  id,
  mode,
  current,
  title,
  onSelect,
  highlight = false,
  children,
}: {
  id: string;
  mode: BackgroundMode;
  current: BackgroundMode;
  title: string;
  onSelect: () => void;
  /** Shown at full strength even when not selected (a file is dragged over it). */
  highlight?: boolean;
  children: ReactNode;
}) {
  const selected = current === mode;
  return (
    <div
      data-setting={id}
      className={`rounded-lg border bg-card px-3.5 py-3 ${
        selected ? "border-primary ring-1 ring-primary dark:border-border" : "border-border"
      }`}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={selected}
        className="mb-2.5 flex w-full items-center gap-2 text-left text-[0.84375rem] font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      >
        <span
          aria-hidden
          className={`inline-block size-4 rounded-full border-[1.5px] ${
            selected ? "border-primary" : "border-input"
          }`}
        >
          {selected ? <span className="m-1 block size-1.5 rounded-full bg-primary" /> : null}
        </span>
        {title}
      </button>
      <div className={selected || highlight ? "" : "opacity-50"}>{children}</div>
    </div>
  );
}

function PatternTiles({
  value,
  onChange,
}: {
  value: PatternKind;
  onChange: (value: PatternKind) => void;
}) {
  const { t } = useTranslation();
  const tiles: { id: PatternKind; label: string; preview: ReactNode }[] = [
    {
      id: "grid",
      label: t("settings.grid"),
      preview: (
        <div
          className="absolute inset-0"
          style={{
            backgroundImage:
              "linear-gradient(color-mix(in srgb, var(--pattern-diamond) 45%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in srgb, var(--pattern-diamond) 45%, transparent) 1px, transparent 1px)",
            backgroundSize: "14px 14px",
          }}
        />
      ),
    },
    {
      id: "default",
      label: t("settings.defaultPattern"),
      preview: (
        <div className="absolute inset-0 flex items-center justify-center">
          <span className="absolute size-5 rotate-45 border border-[var(--pattern-diamond)]" />
          <span className="absolute size-3.5 border border-[var(--pattern-square)]" />
        </div>
      ),
    },
    {
      id: "dots",
      label: t("settings.dots"),
      preview: (
        <div
          className="absolute inset-0"
          style={{
            backgroundImage:
              "radial-gradient(color-mix(in srgb, var(--pattern-diamond) 70%, transparent) 1.2px, transparent 1.6px)",
            backgroundSize: "12px 12px",
          }}
        />
      ),
    },
  ];
  return (
    <div role="radiogroup" aria-label={t("settings.pattern")} className="grid grid-cols-3 gap-2">
      {tiles.map((tile) => {
        const selected = tile.id === value;
        return (
          <button
            key={tile.id}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(tile.id)}
            className="flex flex-col items-center gap-1.5 text-xs text-foreground-variant outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <span
              className={`relative block h-13 w-full overflow-hidden rounded-md border bg-background ${
                selected ? "border-primary ring-1 ring-primary dark:border-border" : "border-border"
              }`}
            >
              {tile.preview}
              {selected ? (
                <span className="absolute top-1 right-1 flex size-4 items-center justify-center rounded-full bg-primary text-primary-foreground">
                  <Check className="size-2.5" strokeWidth={3.5} />
                </span>
              ) : null}
            </span>
            <span className={selected ? "font-medium text-foreground" : ""}>{tile.label}</span>
          </button>
        );
      })}
    </div>
  );
}

function MiniSlider({
  id,
  label,
  value,
  onChange,
}: {
  id: string;
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <div data-setting={id} className="mt-2.5 flex items-center gap-2.5 text-xs">
      <span className="w-14 shrink-0 text-foreground-variant">{label}</span>
      <input
        type="range"
        min={0}
        max={100}
        step={5}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        aria-label={label}
        className="min-w-0 flex-1 accent-primary"
      />
      <span className="w-8 text-right font-mono text-[0.6875rem] text-foreground-variant">
        {value}%
      </span>
    </div>
  );
}

function PreviewCard({ photo }: { photo: BackgroundImageData | null }) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { resolvedTheme } = useTheme();
  const dark = resolvedTheme === "dark";
  const changed = tonesFor("changed", settings.colors.changed, dark);
  const accent = tonesFor("accent", settings.colors.accent, dark);
  const error = tonesFor("error", settings.colors.error, dark);
  const changedFill = isHexColor(settings.colors.changed)
    ? settings.colors.changed
    : changed.main;
  const errorFill = isHexColor(settings.colors.error) ? settings.colors.error : error.main;

  return (
    <div className="sticky top-0 self-start rounded-lg border border-border bg-card p-3">
      <div className="mb-2.5 flex items-center justify-between text-[0.6875rem] font-semibold tracking-[0.06em] text-muted-foreground uppercase">
        {t("settings.preview")}
      </div>
      <div className="relative h-49 overflow-hidden rounded-md border border-border dark:border-surface-highest">
        {settings.backgroundMode === "photo" && photo ? (
          <>
            <div
              className="absolute inset-0"
              style={{
                backgroundImage: `url("${photo.dataUrl}")`,
                backgroundSize: settings.photoFit === "fit" ? "contain" : "cover",
                backgroundPosition: "center",
                backgroundRepeat: "no-repeat",
                filter: `blur(${(settings.photoBlur / 100) * 8}px)`,
              }}
            />
            <div
              className="absolute inset-0"
              style={{
                backgroundColor: `color-mix(in srgb, var(--background) ${settings.photoDarken}%, transparent)`,
              }}
            />
          </>
        ) : (
          <div className="absolute inset-0 bg-background">
            {settings.patternKind === "dots" ? (
              <div
                className="absolute inset-0"
                style={{
                  backgroundImage:
                    "radial-gradient(color-mix(in srgb, var(--pattern-diamond) 70%, transparent) 1.2px, transparent 1.6px)",
                  backgroundSize: "10px 10px",
                  opacity: settings.patternStrength / 100,
                }}
              />
            ) : settings.patternKind === "grid" ? (
              <div
                className="absolute inset-0"
                style={{
                  backgroundImage:
                    "linear-gradient(color-mix(in srgb, var(--pattern-diamond) 45%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in srgb, var(--pattern-diamond) 45%, transparent) 1px, transparent 1px)",
                  backgroundSize: "12px 12px",
                  opacity: settings.patternStrength / 100,
                }}
              />
            ) : (
              <div
                className="absolute inset-0 flex items-center justify-center"
                style={{ opacity: settings.patternStrength / 100 }}
              >
                <span className="absolute size-10 rotate-45 border border-[var(--pattern-diamond)]" />
                <span className="absolute size-7 border border-[var(--pattern-square)]" />
              </div>
            )}
          </div>
        )}
        <div className="absolute inset-x-0 top-0 flex h-4.5 items-center gap-1.5 border-b border-border bg-chrome px-1.5 text-[0.5625rem] font-semibold">
          SavePrism
        </div>
        <div className="absolute top-6 right-2.5 left-2.5 overflow-hidden rounded border border-border bg-card text-[0.6875rem]">
          <div className="flex h-5 items-center bg-table-head px-2 text-[0.5625rem] font-semibold tracking-wide text-muted-foreground uppercase">
            <span className="flex-1">Name</span>
            <span>Value</span>
          </div>
          <PreviewRow label="Gold" value="9,999" color={accent.main} />
          <PreviewRow
            label="Level"
            value="99"
            background={`color-mix(in srgb, ${changedFill} ${dark ? 20 : 9}%, transparent)`}
            color={changed.main}
          />
          <PreviewRow
            label="Health"
            value="-5"
            background={`color-mix(in srgb, ${errorFill} ${dark ? 30 : 8}%, transparent)`}
            color={error.main}
          />
          <PreviewRow label="Hero name" value='"Aria"' />
          <PreviewRow label="Map" value="Forest_02" />
        </div>
      </div>
      <div className="mt-2.5 flex flex-wrap gap-x-3 gap-y-1.5 text-[0.71875rem] text-foreground-variant">
        <Legend color={changed.main} label={t("settings.changedColor")} />
        <Legend color={accent.main} label={t("settings.accentColor")} />
        <Legend color={error.main} label={t("settings.errorColor")} />
      </div>
      <p className="mt-2.5 border-t border-border pt-2.5 text-[0.71875rem] leading-4 text-muted-foreground">
        {t("settings.previewTip")}
      </p>
    </div>
  );
}

function PreviewRow({
  label,
  value,
  background,
  color,
}: {
  label: string;
  value: string;
  background?: string;
  color?: string;
}) {
  return (
    <div
      className="flex h-6 items-center border-t border-border px-2 first:border-t-0"
      style={{ backgroundColor: background }}
    >
      <span className="flex-1 text-foreground-variant">{label}</span>
      <span className="font-mono text-[0.65625rem]" style={{ color }}>
        {value}
      </span>
    </div>
  );
}

function Legend({ color, label }: { color: string; label: string }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <i aria-hidden className="inline-block size-2.5 rounded-full" style={{ backgroundColor: color }} />
      {label}
    </span>
  );
}

