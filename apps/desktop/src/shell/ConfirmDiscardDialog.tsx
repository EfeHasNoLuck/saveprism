import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface ConfirmDiscardDialogProps {
  count: number;
  /**
   * What happens after the discard: another file opens (default), the file or window closes, or
   * the app updates and restarts.
   */
  reason?: "open" | "close" | "update";
  onDiscard: () => void;
  onCancel: () => void;
}

/** Asks before unsaved changes are lost to open another file, close the file or window, or update. */
export function ConfirmDiscardDialog({
  count,
  reason = "open",
  onDiscard,
  onCancel,
}: ConfirmDiscardDialogProps) {
  const { t } = useTranslation();
  const text =
    reason === "close"
      ? {
          title: t("confirm.closeTitle"),
          body: t("confirm.closeBody", { count }),
          discard: t("confirm.closeDiscard"),
        }
      : reason === "update"
        ? {
            title: t("confirm.updateTitle"),
            body: t("confirm.updateBody", { count }),
            discard: t("confirm.updateDiscard"),
          }
        : {
            title: t("confirm.discardOpen", { count }),
            body: t("confirm.discardBody"),
            discard: t("confirm.discard"),
          };
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{text.title}</DialogTitle>
          <DialogDescription>{text.body}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="destructive" onClick={onDiscard}>
            {text.discard}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
