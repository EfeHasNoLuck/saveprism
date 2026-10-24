import { useEffect, useState } from "react";
import { ArchiveRestore } from "lucide-react";
import { useTranslation } from "react-i18next";
import { listBackups, type BackupRow } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ageText, dateTimeText, sizeText } from "./backups";
import { useSettings } from "./context";
import { describeError } from "./errors";

type ListState =
  | { status: "loading" }
  | { status: "ready"; rows: BackupRow[]; now: number }
  | { status: "error"; message: string };

interface BackupsDialogProps {
  fileName: string;
  /** Unsaved changes; a restore throws them away. */
  changeCount: number;
  /** A save or another action is running. */
  busy: boolean;
  onClose: () => void;
  /** Puts the backup `name` back; `when` is its time as the list shows it. */
  onRestore: (name: string, when: string) => Promise<void>;
}

/**
 * The backups of the open file, newest first, each with a Restore button. Restoring asks first: it
 * replaces the file (the current one is backed up before) and drops unsaved changes.
 */
export function BackupsDialog({
  fileName,
  changeCount,
  busy,
  onClose,
  onRestore,
}: BackupsDialogProps) {
  const { t, i18n } = useTranslation();
  const { settings } = useSettings();
  const language = i18n.resolvedLanguage ?? "en";
  const [list, setList] = useState<ListState>({ status: "loading" });
  /** The backup the confirmation asks about, else null. */
  const [asking, setAsking] = useState<BackupRow | null>(null);

  useEffect(() => {
    let cancelled = false;
    listBackups()
      .then((rows) => !cancelled && setList({ status: "ready", rows, now: Date.now() }))
      .catch((error: unknown) => {
        if (cancelled) return;
        const { reason, fix } = describeError(error);
        setList({
          status: "error",
          message: `${t("backups.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  const askingWhen = asking ? dateTimeText(asking.createdMs, language) : "";

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("backups.title")}</DialogTitle>
          <DialogDescription>
            {t("backups.description", { name: fileName })} {t("backups.retention", { count: settings.keepBackups })}
          </DialogDescription>
        </DialogHeader>

        {list.status === "loading" ? (
          <p className="py-4 text-center text-[0.8125rem] text-muted-foreground">{t("common.loading")}</p>
        ) : list.status === "error" ? (
          <p role="alert" className="py-2 text-[0.8125rem] text-destructive">
            {list.message}
          </p>
        ) : list.rows.length === 0 ? (
          <p className="rounded-sm bg-surface-high px-3 py-4 text-center text-[0.8125rem] text-muted-foreground">
            {t("backups.empty")}
          </p>
        ) : (
          <ul className="dense-scroll -mx-1 flex max-h-80 flex-col gap-1 overflow-y-auto px-1">
            {list.rows.map((row) => (
              <li
                key={row.name}
                className="flex items-center gap-3 rounded-sm bg-surface-high px-2.5 py-1.5"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-xs tabular-nums">
                    {dateTimeText(row.createdMs, language)}
                  </p>
                  <p className="truncate text-[0.6875rem] text-muted-foreground">
                    {ageText(row.createdMs, list.now, language)} · {sizeText(row.byteSize, language)}
                  </p>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="shrink-0"
                  disabled={busy}
                  onClick={() => setAsking(row)}
                >
                  <ArchiveRestore />
                  {t("backups.restore")}
                </Button>
              </li>
            ))}
          </ul>
        )}

        {asking ? (
          <Dialog open onOpenChange={(open) => (open ? undefined : setAsking(null))}>
            <DialogContent className="max-w-md">
              <DialogHeader>
                <DialogTitle>{t("backups.confirmTitle")}</DialogTitle>
                <DialogDescription>
                  {t("backups.confirmBody", { name: fileName, time: askingWhen })}
                </DialogDescription>
              </DialogHeader>
              {changeCount > 0 ? (
                <p className="text-[0.8125rem] font-medium text-destructive">
                  {t("backups.confirmLoss", { count: changeCount })}
                </p>
              ) : null}
              <DialogFooter>
                <Button variant="ghost" disabled={busy} onClick={() => setAsking(null)}>
                  {t("common.cancel")}
                </Button>
                <Button
                  variant={changeCount > 0 ? "destructive" : "default"}
                  disabled={busy}
                  // On success the app closes the whole list; after an error (a toast says why)
                  // the list stays.
                  onClick={() => {
                    void onRestore(asking.name, askingWhen).then(() => setAsking(null));
                  }}
                >
                  {busy ? t("backups.restoring") : t("backups.restore")}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
