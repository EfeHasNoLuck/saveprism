import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
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
import { useSettings } from "./context";
import { InfoTip } from "./InfoTip";
import type { SaveDefault } from "./settings";

const LANGUAGE_NAMES: Record<Language, string> = { en: "English", tr: "Türkçe" };
const THEMES: ThemePreference[] = ["system", "light", "dark"];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">{title}</h3>
      <div className="flex flex-col gap-3">{children}</div>
    </section>
  );
}

function Row({ label, info, children }: { label: string; info?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 text-sm">
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
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg bg-muted p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
          className={
            option.value === value
              ? "rounded-md bg-background px-3 py-1 text-sm text-foreground shadow-sm outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
              : "rounded-md px-3 py-1 text-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
          }
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** Settings as a centered modal (dimmed, blurred backdrop; X, Esc and outside click close it). */
export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const { t, i18n } = useTranslation();
  const { settings, update } = useSettings();
  const { preference, setPreference } = useTheme();
  const language: Language = i18n.resolvedLanguage === "tr" ? "tr" : "en";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl overflow-y-auto">
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
              className="w-40 accent-brand disabled:opacity-50"
            />
          </Row>
        </Section>

        <Section title={t("settings.language")}>
          <Row label={t("settings.language")}>
            <Segmented
              label={t("settings.language")}
              value={language}
              options={SUPPORTED_LANGUAGES.map((value) => ({
                value,
                label: LANGUAGE_NAMES[value],
              }))}
              onChange={(value) => void i18n.changeLanguage(value)}
            />
          </Row>
        </Section>

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
        </Section>

        <Section title={t("settings.recentSection")}>
          <Row label={t("settings.showRecent")} info={t("settings.showRecentInfo")}>
            <Switch
              checked={settings.showRecent}
              onCheckedChange={(checked) => update({ showRecent: checked })}
              aria-label={t("settings.showRecent")}
            />
          </Row>
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
      </DialogContent>
    </Dialog>
  );
}
