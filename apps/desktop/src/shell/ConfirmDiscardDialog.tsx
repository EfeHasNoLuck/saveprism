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
  onDiscard: () => void;
  onCancel: () => void;
}

/** Asks before unsaved changes are thrown away to open another file. */
export function ConfirmDiscardDialog({ count, onDiscard, onCancel }: ConfirmDiscardDialogProps) {
  const { t } = useTranslation();
  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("confirm.discardOpen", { count })}</DialogTitle>
          <DialogDescription>{t("confirm.discardBody")}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="destructive" onClick={onDiscard}>
            {t("confirm.discard")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
