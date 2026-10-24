import { useEffect, useState, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { ExternalLink } from "lucide-react";
import { useTranslation } from "react-i18next";
import { coreVersion, noticesAvailable, openNotices } from "@/api/commands";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useNotify } from "./context";
import { describeError } from "./errors";
import { Logo } from "./Logo";

/** Shown while a version is not known (not running inside the desktop shell, or still loading). */
const UNKNOWN = "—";

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex items-baseline justify-between gap-4 text-[0.8125rem]">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono text-xs">{children}</dd>
    </div>
  );
}

interface AboutDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** App and core version, the license, where files and backups stay, and the third-party notices. */
export function AboutDialog({ open, onOpenChange }: AboutDialogProps) {
  const { t } = useTranslation();
  const { notify } = useNotify();
  const [version, setVersion] = useState(UNKNOWN);
  const [core, setCore] = useState(UNKNOWN);
  const [notices, setNotices] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    getVersion()
      .then((value) => !cancelled && setVersion(value))
      .catch(() => undefined);
    coreVersion()
      .then((value) => !cancelled && setCore(value))
      .catch(() => undefined);
    noticesAvailable()
      .then((value) => !cancelled && setNotices(value))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [open]);

  const showNotices = () => {
    openNotices().catch((error: unknown) => {
      const { reason, fix } = describeError(error);
      notify({
        kind: "error",
        title: t("about.noticesFailed"),
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    });
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-sm gap-4">
        <DialogHeader>
          <div className="flex items-center gap-2">
            <Logo className="size-5 shrink-0" />
            <DialogTitle>{t("about.title")}</DialogTitle>
          </div>
          <DialogDescription className="sr-only">{t("about.title")}</DialogDescription>
        </DialogHeader>

        <dl className="flex flex-col gap-1.5">
          <Fact label={t("about.version")}>{version}</Fact>
          <Fact label={t("about.core")}>{core}</Fact>
          <Fact label={t("about.license")}>{t("about.licenseValue")}</Fact>
        </dl>

        <p className="text-[0.8125rem] text-muted-foreground">{t("about.privacy")}</p>

        {notices ? (
          <button
            type="button"
            onClick={showNotices}
            className="inline-flex w-fit items-center gap-1 rounded-sm text-[0.8125rem] text-primary underline-offset-4 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            {t("about.notices")}
            <ExternalLink className="size-3.5" aria-hidden="true" />
          </button>
        ) : (
          <p className="text-[0.8125rem] text-muted-foreground">{t("about.notices")}</p>
        )}
      </DialogContent>
    </Dialog>
  );
}
