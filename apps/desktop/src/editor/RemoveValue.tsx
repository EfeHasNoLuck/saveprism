import { Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { useEditorContext } from "./context";

/**
 * The remove action of a value or entry. A value that is already in the file asks first; the
 * caller leaves the action out for a value added in this session (its undo takes it out).
 */
export function RemoveValueButton({
  path,
  name,
  className,
}: {
  path: string;
  /** Name shown in the question ("Potion", "nickname"). */
  name: string;
  className?: string;
}) {
  const { t } = useTranslation("editor");
  const { t: tc } = useTranslation();
  const { removeValue } = useEditorContext();
  const [open, setOpen] = useState(false);
  const [running, setRunning] = useState(false);

  async function remove() {
    if (running) return;
    setRunning(true);
    const ok = await removeValue(path);
    setRunning(false);
    if (ok) setOpen(false);
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          aria-label={t("remove.action", { name })}
          title={t("remove.action", { name })}
          className={className}
          disabled={running}
        >
          <Trash2 />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t("remove.title", { name })}</DialogTitle>
          <DialogDescription>{t("remove.body", { name })}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={() => setOpen(false)}>
            {tc("common.cancel")}
          </Button>
          <Button variant="destructive" size="sm" disabled={running} onClick={() => void remove()}>
            <Trash2 />
            {t("remove.confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
