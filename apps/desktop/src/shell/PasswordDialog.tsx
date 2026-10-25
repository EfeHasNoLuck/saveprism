import { useEffect, useRef, useState, type FormEvent } from "react";
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
  /** True when the last try searched a picked game folder and found no key there. */
  folderTried: boolean;
  onSubmit: (password: string) => void;
  /** Let the player pick the game's install folder so its own files can be searched. */
  onPickFolder: () => void;
  onCancel: () => void;
}

/** Asks for the password of an encrypted file. The password lives in this component only. */
export function PasswordDialog({
  name,
  wrong,
  folderTried,
  onSubmit,
  onPickFolder,
  onCancel,
}: PasswordDialogProps) {
  const { t } = useTranslation();
  const [password, setPassword] = useState("");
  const input = useRef<HTMLInputElement>(null);

  // A wrong password or an empty game folder leaves focus on a button: give it back to the field.
  useEffect(() => {
    input.current?.focus();
  }, [wrong, folderTried]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (password === "") return;
    onSubmit(password);
    setPassword("");
    input.current?.focus();
  };

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onCancel())}>
      <DialogContent className="max-w-md">
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("password.title")}</DialogTitle>
            <DialogDescription>{t("password.body", { name })}</DialogDescription>
          </DialogHeader>
          <label className="flex flex-col gap-1.5 text-[0.8125rem]">
            {t("password.label")}
            <input
              ref={input}
              type="password"
              autoFocus
              autoComplete="off"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              aria-invalid={wrong}
              className="h-8 rounded-sm border border-input bg-field px-2.5 text-[0.8125rem] outline-none focus-visible:border-ring aria-invalid:border-destructive"
            />
          </label>
          {wrong ? (
            <p role="alert" className="text-[0.8125rem] text-destructive">
              {t("password.wrong")}
            </p>
          ) : null}
          {folderTried && !wrong ? (
            <p role="status" className="text-[0.8125rem] text-muted-foreground">
              {t("password.folderEmpty")}
            </p>
          ) : null}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onPickFolder}>
              {t("password.pickFolder")}
            </Button>
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
