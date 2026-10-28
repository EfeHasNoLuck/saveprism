import { useState, type ComponentType } from "react";
import {
  ChevronDown,
  ChevronUp,
  ClipboardPaste,
  FolderOpen,
  FolderSearch,
  HardDrive,
  HelpCircle,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Upload,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { FAQ_ITEMS, type FaqIcon } from "./faq";

interface StartScreenProps {
  dragging: boolean;
  onOpenFile: () => void;
  /** Opens the dialog that takes a pasted save code. */
  onPasteCode: () => void;
  /** Opens the dialog that lists the usual save folders of each engine. */
  onWhereSaves: () => void;
}

const FAQ_ICONS: Record<FaqIcon, ComponentType<{ className?: string }>> = {
  shield: ShieldCheck,
  games: HardDrive,
  help: HelpCircle,
  ai: Sparkles,
  undo: RotateCcw,
};

export function StartScreen({
  dragging,
  onOpenFile,
  onPasteCode,
  onWhereSaves,
}: StartScreenProps) {
  const { t } = useTranslation();
  const [openId, setOpenId] = useState<string | null>("original");

  return (
    <div className="dense-scroll relative z-10 flex flex-1 flex-col items-center overflow-y-auto px-6 py-10">
      <div className="flex w-full max-w-[640px] flex-col">
        <div
          data-tour="drop"
          className={cn(
            "flex flex-col items-center gap-3 rounded-md border border-dashed px-8 pt-[30px] pb-[22px] text-center transition-colors",
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
          <div className="mt-1.5 flex w-full flex-wrap items-center justify-center gap-1.5 border-t border-border pt-3 text-[0.8125rem] text-muted-foreground">
            <span>{t("start.notSure")}</span>
            <button
              type="button"
              data-tour="where"
              onClick={onWhereSaves}
              className="inline-flex items-center gap-1.5 rounded-sm font-medium text-primary outline-none hover:text-primary/80 focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <FolderSearch className="size-[15px]" aria-hidden="true" />
              {t("start.whereLink")}
            </button>
          </div>
        </div>

        <p className="mt-3.5 flex items-center justify-center gap-2 text-[0.8125rem] font-medium text-foreground-variant">
          <ShieldCheck className="size-4 text-success" aria-hidden="true" />
          {t("start.slogan")}
        </p>

        <h2 className="mt-6 mb-2.5 text-[15px] font-semibold tracking-tight">
          {t("start.faq.title")}
        </h2>
        <div className="overflow-hidden rounded-md border border-border bg-card dark:border-[#1d2d42]">
          {FAQ_ITEMS.map((item, index) => {
            const open = openId === item.id;
            const Icon = FAQ_ICONS[item.icon];
            const panelId = `faq-panel-${item.id}`;
            const buttonId = `faq-button-${item.id}`;
            return (
              <div key={item.id} className={cn(index > 0 && "border-t border-border")}>
                <button
                  type="button"
                  id={buttonId}
                  aria-expanded={open}
                  aria-controls={panelId}
                  onClick={() => setOpenId(open ? null : item.id)}
                  className={cn(
                    "flex h-[42px] w-full items-center gap-2.5 px-3.5 text-left text-[13.5px] font-medium outline-none transition-colors hover:bg-surface-container/60 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-inset",
                    open ? "text-primary" : "text-foreground",
                  )}
                >
                  <Icon
                    className={cn(
                      "size-4 shrink-0",
                      open ? "text-primary" : "text-muted-foreground",
                    )}
                  />
                  {t(item.questionKey)}
                  {open ? (
                    <ChevronUp className="ml-auto size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                  ) : (
                    <ChevronDown className="ml-auto size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
                  )}
                </button>
                {open ? (
                  <div
                    id={panelId}
                    role="region"
                    aria-labelledby={buttonId}
                    className="px-3.5 pb-3.5 pl-10 text-[0.8125rem] leading-[19px] text-foreground-variant"
                  >
                    {t(item.answerKey)
                      .split("\n")
                      .map((line, lineIndex) => (
                        <p key={lineIndex} className={lineIndex > 0 ? "mt-1.5" : undefined}>
                          {line}
                        </p>
                      ))}
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
