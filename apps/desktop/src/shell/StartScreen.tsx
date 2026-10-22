import { useState } from "react";
import { ChevronDown, FileText, FolderOpen, Upload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useSettings } from "./context";
import type { RecentFile } from "./recent";

interface StartScreenProps {
  dragging: boolean;
  recent: RecentFile[];
  onOpenFile: () => void;
  onOpenRecent: (path: string) => void;
}

export function StartScreen({ dragging, recent, onOpenFile, onOpenRecent }: StartScreenProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [faqOpen, setFaqOpen] = useState(false);
  const showRecent = settings.showRecent && recent.length > 0;

  return (
    <div className="relative z-10 flex flex-1 flex-col items-center overflow-y-auto px-6 py-10">
      <div className="flex w-full max-w-2xl flex-1 flex-col justify-center gap-8">
        <div
          className={cn(
            "flex flex-col items-center gap-4 rounded-xl border-2 border-dashed px-8 py-14 text-center transition-colors",
            dragging ? "border-brand bg-brand/10" : "border-border bg-card",
          )}
        >
          <Upload className="size-8 text-brand" aria-hidden="true" />
          <h1 className="text-xl font-semibold">
            {dragging ? t("start.dropActive") : t("empty.title")}
          </h1>
          <Button size="lg" onClick={onOpenFile}>
            <FolderOpen />
            {t("start.openFile")}
          </Button>
          <p className="text-sm text-muted-foreground">{t("start.privacy")}</p>
        </div>

        {showRecent ? (
          <section aria-label={t("start.recent")} className="flex flex-col gap-2">
            <h2 className="text-sm font-medium text-muted-foreground">{t("start.recent")}</h2>
            <ul className="flex flex-col gap-1">
              {recent.map((item) => (
                <li key={item.path}>
                  <button
                    type="button"
                    onClick={() => onOpenRecent(item.path)}
                    className="flex w-full items-center gap-3 rounded-lg bg-card px-3 py-2 text-left outline-none hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50"
                  >
                    <FileText className="size-4 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm">{item.fileName}</span>
                      <span className="block truncate text-xs text-muted-foreground">
                        {item.path}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </div>

      <div className="mt-8 w-full max-w-2xl text-sm">
        <button
          type="button"
          aria-expanded={faqOpen}
          onClick={() => setFaqOpen((open) => !open)}
          className="inline-flex items-center gap-1 rounded-md text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          {t("start.faqLink")}
          <ChevronDown className={cn("size-4 transition-transform", faqOpen && "rotate-180")} />
        </button>
        {/* Always laid out (hidden while closed), so opening it does not move the card above. */}
        <ul
          className={cn(
            "mt-2 flex list-disc flex-col gap-1 pl-5 text-muted-foreground",
            !faqOpen && "invisible",
          )}
        >
          <li>{t("start.faqRpgMaker")}</li>
          <li>{t("start.faqUnity")}</li>
          <li>{t("start.faqUnreal")}</li>
        </ul>
      </div>
    </div>
  );
}
