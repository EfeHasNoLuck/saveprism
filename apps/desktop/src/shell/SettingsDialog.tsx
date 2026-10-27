import { useState, type ReactNode } from "react";
import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { mcpCommand } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Switch } from "@/components/ui/switch";
import { SUPPORTED_LANGUAGES, type Language } from "@/i18n";
import { useTheme, type ThemePreference } from "@/theme";
import { COLOR_PRESETS, type ColorGroup } from "./colors";
import { useNotify, useSettings } from "./context";
import { InfoTip } from "@/components/ui/info-tip";
import { clampKeepBackups, type SaveDefault, type TextSize } from "./settings";
import { mcpSetupJson } from "./setup";

const LANGUAGE_NAMES: Record<Language, string> = { en: "English", tr: "Türkçe" };
const THEMES: ThemePreference[] = ["system", "light", "dark"];
const TEXT_SIZES: TextSize[] = ["small", "normal", "large"];

/**
 * The number of backups to keep. The typed text is kept as a draft and stored only on Enter or
 * when the box loses focus, so a half-typed number (backspacing "10" to "1") never trims backups.
 */
function BackupCountInput({
  value,
  label,
  onCommit,
}: {
  value: number;
  label: string;
  onCommit: (value: number) => void;
}) {
  // The caller keys this box by `value`, so a stored change starts a fresh draft.
  const [draft, setDraft] = useState(String(value));
  const commit = () => {
    const next = draft.trim() === "" ? value : clampKeepBackups(Number(draft));
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return (
    <input
      type="number"
      min={1}
      max={50}
      step={1}
      value={draft}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={commit}
      onKeyDown={(event) => {
        if (event.key === "Enter") commit();
      }}
      aria-label={label}
      className="h-6.5 w-16 rounded-sm border border-input/70 bg-field px-1.5 text-right font-mono text-xs text-foreground tabular-nums outline-none focus-visible:border-ring dark:border-transparent dark:focus-visible:border-ring"
    />
  );
}

/** The shortcuts of the app (see `shortcuts.ts`); a row lists every key that does the same. */
const SHORTCUTS: { action: string; keys: string[] }[] = [
  { action: "open", keys: ["Ctrl+O"] },
  { action: "save", keys: ["Ctrl+S"] },
  { action: "saveAs", keys: ["Ctrl+Shift+S"] },
  { action: "undo", keys: ["Ctrl+Z"] },
  { action: "redo", keys: ["Ctrl+Y", "Ctrl+Shift+Z"] },
  { action: "find", keys: ["Ctrl+F"] },
];

/** A settings group. `aside` puts a single control on the heading's line (no repeated label). */
function Section({
  title,
  aside,
  children,
}: {
  title: string;
  aside?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-4">
        <h3 className="text-[0.6875rem] font-medium tracking-wider text-muted-foreground uppercase">{title}</h3>
        {aside}
      </div>
      {children ? <div className="flex flex-col gap-3">{children}</div> : null}
    </section>
  );
}

function Row({ label, info, children }: { label: string; info?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 text-[0.8125rem]">
      <span className="flex items-center gap-1">
        {label}
        {info ? <InfoTip text={info} /> : null}
      </span>
      {children}
    </div>
  );
}

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

/** Four small colour swatches for one colour group; the chosen one has a ring. */
function ColorSwatches({
  group,
  value,
  onChange,
  label,
}: {
  group: ColorGroup;
  value: string;
  onChange: (value: string) => void;
  label: string;
}) {
  const { t } = useTranslation();
  const { resolvedTheme } = useTheme();
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex items-center gap-1.5">
      {COLOR_PRESETS[group].map((preset) => {
        const name = t(`settings.colorPresets.${group}.${preset.id}`);
        const selected = preset.id === value;
        return (
          <button
            key={preset.id}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={name}
            title={name}
            onClick={() => onChange(preset.id)}
            className={
              selected
                ? "rounded-full p-0.5 ring-2 ring-ring outline-none"
                : "rounded-full p-0.5 ring-2 ring-transparent outline-none hover:ring-border focus-visible:ring-ring/60"
            }
          >
            <span
              aria-hidden
              className="block size-4 rounded-full ring-1 ring-black/15"
              style={{
                backgroundColor:
                  (resolvedTheme === "dark" ? preset.dark : preset.light).main,
              }}
            />
          </button>
        );
      })}
    </div>
  );
}

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Opens the About dialog (the caller closes this one). */
  onOpenAbout: () => void;
}

/** Settings as a centered modal (dimmed, blurred backdrop; X, Esc and outside click close it). */
export function SettingsDialog({ open, onOpenChange, onOpenAbout }: SettingsDialogProps) {
  const { t, i18n } = useTranslation();
  const { settings, update } = useSettings();
  const { notify } = useNotify();
  const { preference, setPreference } = useTheme();
  const language: Language = i18n.resolvedLanguage === "tr" ? "tr" : "en";

  /** Copies the MCP setup of this install; a clipboard that refuses gets the error toast. */
  async function copySetup() {
    try {
      const exe = await mcpCommand();
      await navigator.clipboard.writeText(mcpSetupJson(exe));
      notify({ kind: "saved", title: t("toast.setupCopied") });
    } catch {
      notify({ kind: "error", title: t("toast.copySetupFailed") });
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg gap-5 overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
          <DialogDescription className="sr-only">{t("settings.title")}</DialogDescription>
        </DialogHeader>

        <Section title={t("settings.appearance")}>
          <Row label={t("settings.theme")}>
            <Segmented
              label={t("settings.theme")}
              value={preference}
              options={THEMES.map((value) => ({ value, label: t(`theme.${value}`) }))}
              onChange={setPreference}
            />
          </Row>
          <Row label={t("settings.textSize")}>
            <Segmented<TextSize>
              label={t("settings.textSize")}
              value={settings.textSize}
              options={TEXT_SIZES.map((value) => ({
                value,
                label: t(`settings.textSizes.${value}`),
              }))}
              onChange={(value) => update({ textSize: value })}
            />
          </Row>
          <Row label={t("settings.pattern")} info={t("settings.patternInfo")}>
            <Switch
              checked={settings.patternOn}
              onCheckedChange={(checked) => update({ patternOn: checked })}
              aria-label={t("settings.pattern")}
            />
          </Row>
          <Row label={t("settings.patternStrength")}>
            <input
              type="range"
              min={0}
              max={100}
              step={5}
              value={settings.patternStrength}
              disabled={!settings.patternOn}
              onChange={(event) => update({ patternStrength: Number(event.target.value) })}
              aria-label={t("settings.patternStrength")}
              className="w-40 accent-primary disabled:opacity-50"
            />
          </Row>
        </Section>

        <Section title={t("settings.colors")}>
          <Row label={t("settings.changedColor")}>
            <ColorSwatches
              group="changed"
              label={t("settings.changedColor")}
              value={settings.colors.changed}
              onChange={(changed) =>
                update({ colors: { ...settings.colors, changed } })
              }
            />
          </Row>
          <Row label={t("settings.accentColor")}>
            <ColorSwatches
              group="accent"
              label={t("settings.accentColor")}
              value={settings.colors.accent}
              onChange={(accent) =>
                update({ colors: { ...settings.colors, accent } })
              }
            />
          </Row>
          <Row label={t("settings.errorColor")}>
            <ColorSwatches
              group="error"
              label={t("settings.errorColor")}
              value={settings.colors.error}
              onChange={(error) =>
                update({ colors: { ...settings.colors, error } })
              }
            />
          </Row>
        </Section>

        <Section
          title={t("settings.language")}
          aside={
            <Segmented
              label={t("settings.language")}
              value={language}
              options={SUPPORTED_LANGUAGES.map((value) => ({
                value,
                label: LANGUAGE_NAMES[value],
              }))}
              onChange={(value) => void i18n.changeLanguage(value)}
            />
          }
        />

        <Section title={t("settings.saving")}>
          <Row label={t("settings.defaultAction")} info={t("settings.defaultActionInfo")}>
            <Segmented<SaveDefault>
              label={t("settings.defaultAction")}
              value={settings.saveDefault}
              options={[
                { value: "saveAs", label: t("settings.saveAsNew") },
                { value: "overwrite", label: t("settings.overwriteOriginal") },
              ]}
              onChange={(value) => update({ saveDefault: value })}
            />
          </Row>
          <Row label={t("settings.keepBackups")} info={t("settings.keepBackupsInfo")}>
            <BackupCountInput
              key={settings.keepBackups}
              value={settings.keepBackups}
              label={t("settings.keepBackups")}
              onCommit={(keepBackups) => update({ keepBackups })}
            />
          </Row>
        </Section>

        <Section title={t("settings.aiTools")}>
          <p className="text-[0.8125rem] text-muted-foreground">{t("settings.aiToolsInfo")}</p>
          <div>
            <Button variant="secondary" size="sm" onClick={() => void copySetup()}>
              {t("settings.copySetup")}
            </Button>
          </div>
        </Section>

        <Section title={t("settings.notificationsSection")}>
          <Row label={t("settings.showNotifications")} info={t("settings.showNotificationsInfo")}>
            <Switch
              checked={settings.showNotifications}
              onCheckedChange={(checked) => update({ showNotifications: checked })}
              aria-label={t("settings.showNotifications")}
            />
          </Row>
        </Section>

        <Section title={t("shortcuts.title")}>
          <dl className="grid grid-flow-col grid-cols-2 grid-rows-3 gap-x-6 gap-y-1.5">
            {SHORTCUTS.map(({ action, keys }) => (
              <div key={action} className="flex items-center justify-between gap-2 text-[0.8125rem]">
                <dt className="min-w-0 truncate">{t(`shortcuts.${action}`)}</dt>
                <dd className="flex shrink-0 items-center gap-1">
                  {keys.map((key) => (
                    <kbd
                      key={key}
                      className="rounded-sm bg-surface-high px-1.5 py-0.5 font-mono text-[0.6875rem] text-foreground-variant"
                    >
                      {key}
                    </kbd>
                  ))}
                </dd>
              </div>
            ))}
          </dl>
        </Section>

        <div className="-mb-1 flex border-t border-border pt-3">
          <Button variant="ghost" size="sm" className="text-foreground-variant" onClick={onOpenAbout}>
            <Info />
            {t("settings.about")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
