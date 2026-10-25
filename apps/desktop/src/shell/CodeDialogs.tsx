import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { ClipboardPaste, Copy } from "lucide-react";
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
import { ConfirmDiscardDialog } from "./ConfirmDiscardDialog";
import { describeError } from "./errors";

const TEXTAREA =
  "dense-scroll h-44 w-full resize-none rounded-sm border border-input bg-field px-2.5 py-2 font-mono text-xs leading-5 break-all outline-none focus-visible:border-ring aria-invalid:border-destructive";

/** What opening a pasted code gave: the dialog closes on success and shows the error otherwise. */
export type OpenCodeResult = { ok: true } | { ok: false; error: unknown };

interface PasteCodeDialogProps {
  /** Text to start with (a Ctrl+V on the start screen), else empty. */
  initialText?: string;
  /** Unsaved changes of the open file: opening the code asks before it throws them away. */
  changeCount: number;
  onOpen: (text: string) => Promise<OpenCodeResult>;
  onClose: () => void;
}

/** Takes a save code the player pastes and opens it. A code that does not open shows why here. */
export function PasteCodeDialog({ initialText = "", changeCount, onOpen, onClose }: PasteCodeDialogProps) {
  const { t } = useTranslation();
  const fieldId = useId();
  const [text, setText] = useState(initialText);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  /** The refusal of the last Open, with the text it was for. */
  const [refused, setRefused] = useState<{ text: string; error: unknown } | null>(null);
  const empty = text.trim() === "";

  const run = async () => {
    setBusy(true);
    setRefused(null);
    const result = await onOpen(text);
    setBusy(false);
    if (!result.ok) setRefused({ text, error: result.error });
  };

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (empty || busy) return;
    if (changeCount > 0) setAsking(true);
    else void run();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) submit(event);
  };

  const shown = refused !== null && refused.text === text ? describeError(refused.error, t) : null;

  return (
    <Dialog open onOpenChange={(open) => (open || busy ? undefined : onClose())}>
      <DialogContent className="max-w-xl">
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("paste.title")}</DialogTitle>
            <DialogDescription>{t("paste.body")}</DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1.5">
            <label htmlFor={fieldId} className="sr-only">
              {t("paste.label")}
            </label>
            <textarea
              id={fieldId}
              autoFocus
              spellCheck={false}
              autoComplete="off"
              value={text}
              placeholder={t("paste.placeholder")}
              onChange={(event) => setText(event.target.value)}
              onKeyDown={onKeyDown}
              aria-invalid={shown !== null}
              className={TEXTAREA}
            />
            <div className="min-h-5 text-xs" aria-live="polite">
              {shown !== null ? (
                <p role="alert" className="text-destructive">
                  {`${t("paste.failed")} ${shown.reason} ${shown.fix}`.trim()}
                </p>
              ) : (
                <p className="text-muted-foreground">{t("paste.note")}</p>
              )}
            </div>
          </div>
          <DialogFooter>
            <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
              {t("common.cancel")}
            </Button>
            <Button type="submit" disabled={empty || busy}>
              <ClipboardPaste />
              {busy ? t("paste.opening") : t("paste.open")}
            </Button>
          </DialogFooter>
        </form>
        {asking ? (
          <ConfirmDiscardDialog
            count={changeCount}
            onCancel={() => setAsking(false)}
            onDiscard={() => {
              setAsking(false);
              void run();
            }}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

interface CodeTextDialogProps {
  code: string;
  onClose: () => void;
}

/**
 * Shows a save code to copy by hand, for when the clipboard could not be written. The text is
 * read-only and selected, so Ctrl+C copies it.
 */
export function CodeTextDialog({ code, onClose }: CodeTextDialogProps) {
  const { t } = useTranslation();
  const fieldId = useId();
  const fieldRef = useRef<HTMLTextAreaElement>(null);
  const [copied, setCopied] = useState(false);

  const selectAll = () => {
    fieldRef.current?.focus();
    fieldRef.current?.select();
  };

  useEffect(() => {
    selectAll();
  }, []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      selectAll();
    }
  };

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("codeText.title")}</DialogTitle>
          <DialogDescription>{t("codeText.body")}</DialogDescription>
        </DialogHeader>
        <label htmlFor={fieldId} className="sr-only">
          {t("codeText.label")}
        </label>
        <textarea
          id={fieldId}
          ref={fieldRef}
          readOnly
          spellCheck={false}
          value={code}
          onFocus={(event) => event.currentTarget.select()}
          className={TEXTAREA}
        />
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={selectAll}>
            {t("codeText.selectAll")}
          </Button>
          <Button type="button" variant="outline" onClick={() => void copy()}>
            <Copy />
            {copied ? t("common.copied") : t("common.copy")}
          </Button>
          <Button type="button" onClick={onClose}>
            {t("common.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
