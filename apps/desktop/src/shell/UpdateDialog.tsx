import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { progressPercent, type InstallProgress } from "./update";

/**
 * The download and install of an update. It cannot be closed: the app restarts at the end, and a
 * failure closes it and shows a toast instead.
 */
export function UpdateDialog({ progress }: { progress: InstallProgress }) {
  const { t } = useTranslation();
  const installing = progress.phase === "install";
  const percent = installing ? 100 : progressPercent(progress);
  return (
    <Dialog open>
      <DialogContent
        className="max-w-sm"
        showClose={false}
        onEscapeKeyDown={(event) => event.preventDefault()}
        onPointerDownOutside={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogHeader className="pr-0">
          <DialogTitle>{t("update.dialogTitle", { version: progress.version })}</DialogTitle>
          <DialogDescription>
            {installing ? t("update.installing") : t("update.downloading")}
          </DialogDescription>
        </DialogHeader>
        <div className="flex items-center gap-3">
          <div
            role="progressbar"
            aria-label={t("update.dialogTitle", { version: progress.version })}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={percent ?? undefined}
            className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-surface-high"
          >
            <div
              className={
                percent === null
                  ? "h-full w-1/3 animate-pulse rounded-full bg-primary motion-reduce:animate-none"
                  : "h-full rounded-full bg-primary transition-[width] duration-200"
              }
              style={percent === null ? undefined : { width: `${percent}%` }}
            />
          </div>
          {installing ? (
            <LoaderCircle
              className="size-4 shrink-0 animate-spin text-primary motion-reduce:animate-none"
              aria-hidden="true"
            />
          ) : (
            <span className="w-10 shrink-0 text-right font-mono text-xs text-muted-foreground">
              {percent === null ? "" : `${percent} %`}
            </span>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
