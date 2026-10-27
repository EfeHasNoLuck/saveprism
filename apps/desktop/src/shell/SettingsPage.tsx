import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  Bot,
  Bug,
  Check,
  ChevronDown,
  ChevronRight,
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
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  backgroundImage,
  chooseBackground,
  coreVersion,
  defaultPaths,
  mcpCommand,
  noticesAvailable,
  openFolder,
  openNotices,
  setAppFolder,
  setBackupFolder,
  type BackgroundImage as BackgroundImageData,
  type DefaultPaths,
} from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Switch } from "@/components/ui/switch";
import { SUPPORTED_LANGUAGES, type Language } from "@/i18n";
import { LanguageFlag } from "@/i18n/flags";
import { languageName } from "@/i18n/language";
import { useTheme, type ThemePreference } from "@/theme";
import { COLOR_PRESETS, DEFAULT_COLORS, isHexColor, textColorFor, tonesFor, type ColorGroup } from "./colors";
import { useNotify, useSettings } from "./context";
import { describeError } from "./errors";
import {
  clampKeepBackups,
  clampKeepDays,
  type BackgroundMode,
  type PatternKind,
  type PhotoFit,
  type SaveDefault,
} from "./settings";
import { searchSettings, type SettingsTab } from "./settingsSearch";
import { mcpSetupJson } from "./setup";

const COMMUNITY_URL = "https://github.com/huseyincancalti/saveprism/discussions";

const THEMES: ThemePreference[] = ["system", "light", "dark"];

const TABS: { id: SettingsTab; icon: typeof SlidersHorizontal; labelKey: string }[] = [
  { id: "general", icon: SlidersHorizontal, labelKey: "settings.tabs.general" },
  { id: "appearance", icon: Palette, labelKey: "settings.tabs.appearance" },
  { id: "backups", icon: History, labelKey: "settings.tabs.backups" },
  { id: "ai", icon: Sparkles, labelKey: "settings.tabs.ai" },
  { id: "shortcuts", icon: Keyboard, labelKey: "settings.tabs.shortcuts" },
  { id: "about", icon: Info, labelKey: "settings.tabs.about" },
];

/** The shortcuts of the app (see `shortcuts.ts`); a row lists every key that does the same. */
const SHORTCUTS: { action: string; keys: string[] }[] = [
  { action: "open", keys: ["Ctrl+O"] },
  { action: "save", keys: ["Ctrl+S"] },
  { action: "saveAs", keys: ["Ctrl+Shift+S"] },
  { action: "undo", keys: ["Ctrl+Z"] },
  { action: "redo", keys: ["Ctrl+Y", "Ctrl+Shift+Z"] },
  { action: "find", keys: ["Ctrl+F"] },
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
      className="w-48 accent-primary"
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
    <section className="mt-5 first:mt-0">
      <div className="mb-1.5 flex h-6 items-center justify-between gap-3">
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
    <div className="overflow-hidden rounded-lg border border-border bg-card dark:border-[#1d2d42]">
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
      className={`flex min-h-14 items-center gap-4 border-t border-border px-4 py-2.5 first:border-t-0 dark:border-[#1d2d42] ${
        highlight ? "bg-primary/8" : ""
      }`}
    >
      <div className="min-w-0 flex-1">
        <b className="block text-[0.84375rem] leading-5 font-medium">{label}</b>
        {description ? (
          <span className="mt-px block text-xs leading-[17px] text-muted-foreground">
            {description}
          </span>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/** The whole Settings page (layout B): a sidebar with the tabs under the app header. */
export function SettingsPage({ onClose }: { onClose: () => void }) {
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
  const [version, setVersion] = useState("—");
  const [core, setCore] = useState("—");
  const [notices, setNotices] = useState(false);
  const pageRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    defaultPaths()
      .then((value) => !cancelled && setPaths(value))
      .catch(() => undefined);
    getVersion()
      .then((value) => !cancelled && setVersion(value))
      .catch(() => undefined);
    coreVersion()
      .then((value) => !cancelled && setCore(value))
      .catch(() => undefined);
    noticesAvailable()
      .then((value) => !cancelled && setNotices(value))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", listener);
    pageRef.current?.focus();
    return () => window.removeEventListener("keydown", listener);
  }, [onClose]);

  useEffect(() => {
    if (settings.backgroundMode !== "photo") return;
    let cancelled = false;
    backgroundImage(settings.appFolder)
      .then((image) => !cancelled && setPhoto(image))
      .catch(() => !cancelled && setPhoto(null));
    return () => {
      cancelled = true;
    };
  }, [settings.backgroundMode, settings.appFolder, settings.photoName]);

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

  const copySetup = useCallback(async () => {
    try {
      const exe = await mcpCommand();
      await navigator.clipboard.writeText(mcpSetupJson(exe));
      notify({ kind: "saved", title: t("toast.setupCopied") });
    } catch {
      notify({ kind: "error", title: t("toast.copySetupFailed") });
    }
  }, [notify, t]);

  const openCommunity = useCallback(async () => {
    try {
      await openUrl(COMMUNITY_URL);
    } catch {
      window.open(COMMUNITY_URL, "_blank", "noopener,noreferrer");
    }
  }, []);

  const showNotices = useCallback(() => {
    openNotices().catch((error: unknown) => reportError(t("about.noticesFailed"), error));
  }, [reportError, t]);

  const titleKey = TABS.find((item) => item.id === tab)?.labelKey ?? "settings.tabs.general";

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
            onClick={() => void openCommunity()}
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
          <div className="mb-1 flex items-center gap-1 text-xs text-muted-foreground">
            {t("settings.title")}
            <ChevronRight className="size-3" />
            <span>{t(titleKey)}</span>
          </div>
          <h2 className="text-[1.375rem] leading-7 font-semibold tracking-tight">
            {t(titleKey)}
          </h2>
          <p className="mt-0.5 text-xs text-muted-foreground">{t(`settings.sub.${tab}`)}</p>

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
                    className={`grid grid-cols-2 gap-3 ${highlight === "appearance.background" ? "rounded-lg ring-2 ring-primary/40" : ""}`}
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
                    >
                      <div className="flex items-center gap-2.5">
                        <span className="flex h-11 w-18 shrink-0 items-center justify-center overflow-hidden rounded-md border border-input border-dashed text-muted-foreground">
                          {photo ? (
                            <img src={photo.dataUrl} alt="" className="size-full object-cover" />
                          ) : (
                            <ImageIcon className="size-4" />
                          )}
                        </span>
                        <div className="min-w-0 text-xs text-muted-foreground">
                          <b className="block truncate text-[0.78rem] font-medium text-foreground">
                            {photo?.name ?? t("settings.noImage")}
                          </b>
                          {t("settings.imageCopiedHint")}
                        </div>
                      </div>
                      <div
                        data-setting="appearance.fit"
                        className="mt-2.5 flex items-center justify-between gap-2"
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
                  <div className="flex items-start gap-2.5 border-t border-border bg-primary/7 px-4 py-2.5 text-xs text-foreground dark:border-[#1d2d42]">
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
              <div className="rounded-lg border border-border bg-card p-4 dark:border-[#1d2d42]">
                <div className="flex items-start gap-3">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface-high text-primary">
                    <MessageSquare className="size-4" />
                  </span>
                  <div>
                    <b className="block text-sm font-semibold">{t("settings.aiTools")}</b>
                    <p className="mt-1 text-xs leading-[17px] text-muted-foreground">
                      {t("settings.aiToolsInfo")}
                    </p>
                  </div>
                </div>
                <Button variant="secondary" size="sm" className="mt-3" onClick={() => void copySetup()}>
                  {t("settings.copySetup")}
                </Button>
              </div>
              <div className="rounded-lg border border-border bg-card p-4 dark:border-[#1d2d42]">
                <div className="flex items-start gap-3">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-surface-high text-primary">
                    <Bot className="size-4" />
                  </span>
                  <div>
                    <b className="block text-sm font-semibold">{t("settings.mcpTitle")}</b>
                    <p className="mt-1 text-xs leading-[17px] text-muted-foreground">
                      {t("settings.mcpHint")}
                    </p>
                  </div>
                </div>
              </div>
            </div>
          ) : null}

          {tab === "shortcuts" ? (
            <div className="mt-4">
              <Group>
                <div className="flex h-8 items-center border-b border-border px-4 text-[0.6875rem] font-semibold tracking-[0.04em] text-muted-foreground uppercase dark:border-[#1d2d42]">
                  <span className="w-52">{t("settings.action")}</span>
                  <span>{t("settings.shortcutsColumn")}</span>
                </div>
                {SHORTCUTS.map(({ action, keys }) => (
                  <div
                    key={action}
                    className="flex h-10 items-center border-t border-border px-4 text-[0.8125rem] first:border-t-0 dark:border-[#1d2d42]"
                  >
                    <span className="w-52 font-medium">{t(`shortcuts.${action}`)}</span>
                    <span className="flex flex-1 flex-wrap items-center gap-2">
                      {keys.map((key) => (
                        <kbd
                          key={key}
                          className="rounded-sm bg-surface-high px-1.5 py-0.5 font-mono text-[0.6875rem] text-foreground-variant"
                        >
                          {key}
                        </kbd>
                      ))}
                    </span>
                  </div>
                ))}
              </Group>
            </div>
          ) : null}

          {tab === "about" ? (
            <div className="mt-5 flex flex-col items-center">
              <span className="flex size-20 items-center justify-center rounded-[20px] bg-[linear-gradient(160deg,#13294a,#061428)] shadow-[0_10px_30px_-10px_rgba(0,0,0,0.6)]">
                <Users className="size-8 text-[#abc7ff]" />
              </span>
              <h3 className="mt-3.5 text-2xl font-semibold tracking-tight">SavePrism</h3>
              <p className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                {t("about.version")} {version}
                <span className="rounded-full bg-surface-high px-2 py-0.5 text-[0.6875rem] font-semibold">
                  MIT
                </span>
              </p>
              <div className="mt-4 w-[440px] max-w-full overflow-hidden rounded-lg border border-border bg-card dark:border-[#1d2d42]">
                <AboutRow icon={<Gift className="size-3.5" />} label={t("about.version")} value={version} />
                <AboutRow icon={<FileText className="size-3.5" />} label={t("about.core")} value={core} />
                <AboutRow icon={<Scale className="size-3.5" />} label={t("about.license")} value={t("about.licenseValue")} />
                {notices ? (
                  <button
                    type="button"
                    onClick={showNotices}
                    className="flex h-11 w-full items-center gap-3 border-t border-border px-3.5 text-left text-[0.84375rem] outline-none hover:bg-surface-high focus-visible:bg-surface-high dark:border-[#1d2d42]"
                  >
                    <span className="flex size-7 items-center justify-center rounded-md bg-surface-high text-primary">
                      <FileText className="size-3.5" />
                    </span>
                    {t("about.notices")}
                    <ExternalLink className="ml-auto size-3.5 text-muted-foreground" />
                  </button>
                ) : null}
                <AboutLink icon={<Github className="size-3.5" />} label={t("settings.sourceCode")} url="https://github.com/huseyincancalti/saveprism" />
                <AboutLink icon={<Users className="size-3.5" />} label={t("settings.communityThemes")} url={COMMUNITY_URL} />
                <AboutLink icon={<Bug className="size-3.5" />} label={t("settings.reportProblem")} url="https://github.com/huseyincancalti/saveprism/issues" />
              </div>
              <p className="mt-1.5 max-w-md text-center text-xs leading-[17px] text-muted-foreground">
                {t("about.privacy")}
              </p>
              <p className="mt-3 text-[0.71875rem] text-muted-foreground">© 2026 SavePrism contributors</p>
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}

function AboutRow({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className="flex h-11 items-center gap-3 border-t border-border px-3.5 text-[0.84375rem] first:border-t-0 dark:border-[#1d2d42]">
      <span className="flex size-7 items-center justify-center rounded-md bg-surface-high text-primary">
        {icon}
      </span>
      <span>{label}</span>
      <span className="ml-auto font-mono text-xs text-muted-foreground">{value}</span>
    </div>
  );
}

function AboutLink({ icon, label, url }: { icon: ReactNode; label: string; url: string }) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className="flex h-11 items-center gap-3 border-t border-border px-3.5 text-[0.84375rem] outline-none hover:bg-surface-high focus-visible:bg-surface-high dark:border-[#1d2d42]"
    >
      <span className="flex size-7 items-center justify-center rounded-md bg-surface-high text-primary">
        {icon}
      </span>
      {label}
      <ExternalLink className="ml-auto size-3.5 text-muted-foreground" />
    </a>
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
              ? `0 0 0 2px var(--card), 0 0 0 4px ${textColorFor(current)}`
              : undefined,
          }}
        >
          <span
            aria-hidden
            className="flex size-full items-center justify-center rounded-full"
            style={{
              background: custom
                ? current
                : "conic-gradient(#ff5f6d, #ffc371, #f9f871, #6fdba8, #4cd7f6, #7b8cff, #d0bcff, #ff5f6d)",
            }}
          >
            {custom ? (
              <Check className="size-3" style={{ color: textColorFor(current) }} strokeWidth={3.5} />
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

function BackgroundCard({
  id,
  mode,
  current,
  title,
  onSelect,
  children,
}: {
  id: string;
  mode: BackgroundMode;
  current: BackgroundMode;
  title: string;
  onSelect: () => void;
  children: ReactNode;
}) {
  const selected = current === mode;
  return (
    <div
      data-setting={id}
      className={`rounded-lg border bg-card px-3.5 py-3 dark:border-[#1d2d42] ${
        selected ? "border-primary ring-1 ring-primary" : "border-border"
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
      <div className={selected ? "" : "opacity-50"}>{children}</div>
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
                selected ? "border-primary ring-1 ring-primary" : "border-border"
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

  return (
    <div className="sticky top-0 self-start rounded-lg border border-border bg-card p-3 dark:border-[#1d2d42]">
      <div className="mb-2.5 flex items-center justify-between text-[0.6875rem] font-semibold tracking-[0.06em] text-muted-foreground uppercase">
        {t("settings.preview")}
      </div>
      <div className="relative h-49 overflow-hidden rounded-md border border-border dark:border-[#26364a]">
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
              style={{ backgroundColor: `rgba(0, 6, 15, ${settings.photoDarken / 100})` }}
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
        <div className="absolute inset-x-0 top-0 flex h-4.5 items-center gap-1.5 border-b border-border bg-chrome px-1.5 text-[0.5625rem] font-semibold dark:border-[#1d2d42]">
          SavePrism
        </div>
        <div className="absolute top-6 right-2.5 left-2.5 overflow-hidden rounded border border-border bg-card text-[0.6875rem] dark:border-[#1d2d42]">
          <div className="flex h-5 items-center bg-table-head px-2 text-[0.5625rem] font-semibold tracking-wide text-muted-foreground uppercase">
            <span className="flex-1">Name</span>
            <span>Value</span>
          </div>
          <PreviewRow label="Gold" value="9,999" color={accent.main} />
          <PreviewRow
            label="Level"
            value="99"
            background={`color-mix(in srgb, ${changed.main} ${dark ? 20 : 9}%, transparent)`}
            color={changed.main}
          />
          <PreviewRow
            label="Health"
            value="-5"
            background={`color-mix(in srgb, ${error.main} ${dark ? 30 : 8}%, transparent)`}
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
      <p className="mt-2.5 border-t border-border pt-2.5 text-[0.71875rem] leading-4 text-muted-foreground dark:border-[#1d2d42]">
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
      className="flex h-6 items-center border-t border-border px-2 first:border-t-0 dark:border-[#1d2d42]"
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

