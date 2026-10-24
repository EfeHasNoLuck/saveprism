import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * A calm cover over the main area while a big file opens. It sits on top of whatever is shown, so
 * nothing underneath can be clicked until the file is open (or refused).
 */
export function OpeningOverlay({ name }: { name: string }) {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      aria-live="polite"
      className="absolute inset-0 z-20 flex items-center justify-center bg-background/70 backdrop-blur-[1px] animate-in duration-200 fade-in-0"
    >
      <div className="flex max-w-[min(32rem,calc(100%-2rem))] items-center gap-2.5 rounded-md border border-border bg-card px-4 py-2.5 text-[0.8125rem] shadow-lg shadow-black/20">
        <LoaderCircle
          className="size-4 shrink-0 animate-spin text-primary motion-reduce:animate-none"
          aria-hidden="true"
        />
        <span className="min-w-0 truncate">{t("common.opening", { name })}</span>
      </div>
    </div>
  );
}
