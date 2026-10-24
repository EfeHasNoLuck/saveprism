import { useState } from "react";
import { ChevronDown, ClipboardPaste, FileText, FolderOpen, Upload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useSettings } from "./context";
import type { RecentFile } from "./recent";

interface StartScreenProps {
  dragging: boolean;
  recent: RecentFile[];
  onOpenFile: () => void;
  /** Opens the dialog that takes a pasted save code. */
  onPasteCode: () => void;
  onOpenRecent: (path: string) => void;
}

export function StartScreen({
  dragging,
  recent,
  onOpenFile,
  onPasteCode,
  onOpenRecent,
}: StartScreenProps) {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [faqOpen, setFaqOpen] = useState(false);
  const showRecent = settings.showRecent && recent.length > 0;

  return (
    <div className="relative z-10 flex flex-1 flex-col items-center overflow-y-auto px-6 py-10">
      <div className="flex w-full max-w-xl flex-1 flex-col justify-center gap-6">
        <div
          className={cn(
            "flex flex-col items-center gap-3 rounded-md border border-dashed px-8 py-12 text-center transition-colors",
            dragging ? "border-primary bg-primary/10" : "border-input bg-card",
          )}
        >
          <span className="flex size-10 items-center justify-center rounded-md bg-surface-high">
            <Upload className="size-5 text-primary" aria-hidden="true" />
          </span>
          <h1 className="text-xl font-semibold tracking-tight">
            {dragging ? t("start.dropActive") : t("empty.title")}
          </h1>
          <div className="flex flex-wrap items-center justify-center gap-2">
            <Button onClick={onOpenFile}>
              <FolderOpen />
              {t("start.openFile")}
            </Button>
            <Button
              variant="secondary"
              onClick={onPasteCode}
              className="bg-surface-container hover:bg-surface-high"
            >
              <ClipboardPaste />
              {t("start.pasteCode")}
            </Button>
          </div>
          <p className="text-[0.8125rem] text-muted-foreground">{t("start.pasteHint")}</p>
          <p className="text-[0.8125rem] text-muted-foreground">{t("start.privacy")}</p>
        </div>

        {showRecent ? (
          <section aria-label={t("start.recent")} className="flex flex-col gap-2">
            <h2 className="px-1 text-[0.6875rem] font-medium tracking-wider text-muted-foreground uppercase">
              {t("start.recent")}
            </h2>
            <ul className="flex flex-col overflow-hidden rounded-md border border-border bg-card">
              {recent.map((item) => (
                <li key={item.path}>
                  <button
                    type="button"
                    onClick={() => onOpenRecent(item.path)}
                    className="flex w-full items-center gap-3 border-b border-border px-3 py-1.5 text-left outline-none hover:bg-surface-container focus-visible:bg-surface-container"
                  >
                    <FileText className="size-4 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-mono text-xs">{item.fileName}</span>
                      <span className="block truncate text-[0.6875rem] text-muted-foreground">
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

      <div className="mt-6 w-full max-w-xl text-[0.8125rem]">
        <button
          type="button"
          aria-expanded={faqOpen}
          onClick={() => setFaqOpen((open) => !open)}
          className="inline-flex items-center gap-1 rounded-sm text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
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
