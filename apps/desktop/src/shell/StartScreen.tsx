import { ClipboardPaste, FolderOpen, FolderSearch, Upload } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface StartScreenProps {
  dragging: boolean;
  onOpenFile: () => void;
  /** Opens the dialog that takes a pasted save code. */
  onPasteCode: () => void;
  /** Opens the dialog that lists the usual save folders of each engine. */
  onWhereSaves: () => void;
}

export function StartScreen({
  dragging,
  onOpenFile,
  onPasteCode,
  onWhereSaves,
}: StartScreenProps) {
  const { t } = useTranslation();

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
      </div>

      <div className="mt-6 flex w-full max-w-xl flex-col gap-1 text-[0.8125rem]">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
          <button
            type="button"
            onClick={onWhereSaves}
            className="inline-flex items-center gap-1.5 rounded-sm font-medium text-primary outline-none hover:text-primary/80 focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <FolderSearch className="size-4" aria-hidden="true" />
            {t("start.whereLink")}
          </button>
        </div>
      </div>
    </div>
  );
}
