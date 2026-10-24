import { useTranslation } from "react-i18next";
import type { SugarCubeView } from "@/api/commands";
import { InfoTip } from "@/components/ui/info-tip";
import { cn } from "@/lib/utils";
import { useFormatNumber } from "./context";
import { formatDateTime } from "./helpers";
import { Section } from "./parts";
import { ValuesCard } from "./ValuesCard";

/** One line of the Story card: the label on the left, the value (cut to one line) on the right. */
function Fact({
  label,
  value,
  info,
  className,
}: {
  label: string;
  value: string;
  info?: string;
  className?: string;
}) {
  return (
    <div className="flex h-6 min-w-0 items-center gap-3 text-xs">
      <dt className="flex shrink-0 items-center gap-0.5 text-muted-foreground">
        {label}
        {info ? <InfoTip text={info} /> : null}
      </dt>
      <dd className={cn("ml-auto min-w-0 truncate text-right", className)} title={value}>
        {value}
      </dd>
    </div>
  );
}

/** The save's facts: its title, the passage the game resumes at, when it was saved, the moment. */
function StoryCard({ view }: { view: SugarCubeView }) {
  const { t, i18n } = useTranslation("editor");
  const format = useFormatNumber();
  const unknown = t("story.unknown");
  const saved = view.date === null ? null : formatDateTime(view.date, i18n.resolvedLanguage ?? "en");
  return (
    <Section title={t("sections.story")}>
      <dl className="flex flex-col">
        <Fact label={t("story.title")} value={view.title ?? unknown} />
        <Fact
          label={t("story.passage")}
          value={view.passage ?? unknown}
          className={cn(view.passage !== null && "font-medium text-tertiary")}
        />
        <Fact label={t("story.saved")} value={saved ?? unknown} className="tabular-nums" />
        <Fact
          label={t("story.moment")}
          info={t("info.moment")}
          value={t("story.momentOf", {
            current: format(view.index + 1),
            total: format(view.moments),
          })}
          className="font-mono tabular-nums"
        />
      </dl>
    </Section>
  );
}

/** SugarCube (Twine) saves: the Story card beside the story variables. */
export function SugarCube({ view }: { view: SugarCubeView }) {
  const { t } = useTranslation("editor");
  return (
    <div className="grid items-start gap-2.5 md:grid-cols-[16rem_minmax(0,1fr)]">
      <StoryCard view={view} />
      <ValuesCard
        rows={view.variables}
        title={t("sections.storyVariables")}
        info={t("info.storyVariables")}
        addParent={view.addParent}
      />
    </div>
  );
}
