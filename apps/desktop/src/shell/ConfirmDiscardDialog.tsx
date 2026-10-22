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
  /** What happens after the discard: another file opens (default) or the window closes. */
  reason?: "open" | "close";
  onDiscard: () => void;
  onCancel: () => void;
}

/** Asks before unsaved changes are thrown away to open another file or to close the window. */
export function ConfirmDiscardDialog({
  count,
  reason = "open",
  onDiscard,
  onCancel,
}: ConfirmDiscardDialogProps) {
  const { t } = useTranslation();
  const closing = reason === "close";
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {closing ? t("confirm.closeTitle") : t("confirm.discardOpen", { count })}
          </DialogTitle>
          <DialogDescription>
            {closing ? t("confirm.closeBody", { count }) : t("confirm.discardBody")}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="destructive" onClick={onDiscard}>
            {closing ? t("confirm.closeDiscard") : t("confirm.discard")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
