import { useState, type FormEvent } from "react";
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

interface PasswordDialogProps {
  /** File name shown in the text. */
  name: string;
  /** True when the last password did not open the file. */
  wrong: boolean;
  onSubmit: (password: string) => void;
  onCancel: () => void;
}

/** Asks for the password of an encrypted file. The password lives in this component only. */
export function PasswordDialog({ name, wrong, onSubmit, onCancel }: PasswordDialogProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (password === "") return;
    onSubmit(password);
    setPassword("");
  };

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <DialogContent className="max-w-md">
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("password.title")}</DialogTitle>
            <DialogDescription>{t("password.body", { name })}</DialogDescription>
          </DialogHeader>
          <label className="flex flex-col gap-1.5 text-sm">
            {t("password.label")}
            <input
              type="password"
              autoFocus
              autoComplete="off"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              aria-invalid={wrong}
              className="h-9 rounded-lg border border-input bg-background px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 aria-invalid:border-destructive"
            />
          </label>
          {wrong ? (
            <p role="alert" className="text-sm text-destructive">
              {t("password.wrong")}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" disabled={password === ""}>
              {t("password.submit")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
