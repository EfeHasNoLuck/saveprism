import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import changelog from "../../../../CHANGELOG.md?raw";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { changelogFor } from "./changelog";

const BOLD = /^\*\*(.+)\*\*$/;
const LINK = /^\[([^\]]+)\]\(([^)]+)\)$/;

function openExternal(url: string) {
  openUrl(url).catch(() => window.open(url, "_blank", "noopener,noreferrer"));
}

/** Renders bold text and links without a markdown library. */
function Inline({ text }: { text: string }) {
  const parts = text.split(/(\*\*[^*]+\*\*|\[[^\]]+\]\([^)]+\))/g);
  return (
    <>
      {parts.map((part, index) => {
        const bold = BOLD.exec(part);
        if (bold) return <strong key={index}>{bold[1]}</strong>;
        const link = LINK.exec(part);
        if (link) {
          const url = link[2];
          if (!/^https?:\/\//i.test(url)) return <span key={index}>{link[1]}</span>;
          return (
            <button
              key={index}
              type="button"
              onClick={() => openExternal(url)}
              className="rounded-sm font-medium text-primary underline-offset-4 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              {link[1]}
            </button>
          );
        }
        return <span key={index}>{part}</span>;
      })}
    </>
  );
}

/** The release notes of the running version, read from `CHANGELOG.md` at build time. */
export function WhatsNewDialog({ version, onClose }: { version: string; onClose: () => void }) {
  const { t } = useTranslation();
  const lines = changelogFor(changelog, version) ?? [];
  const blocks: ReactNode[] = [];
  let bullets: string[] = [];

  const flushBullets = () => {
    if (bullets.length === 0) return;
    blocks.push(
      <ul key={`list-${blocks.length}`} className="my-1.5 list-disc space-y-1 pl-5">
        {bullets.map((item) => (
          <li key={item}>
            <Inline text={item} />
          </li>
        ))}
      </ul>,
    );
    bullets = [];
  };

  for (const line of lines) {
    const text = line.trim();
    if (text === "") {
      flushBullets();
    } else if (text.startsWith("### ")) {
      flushBullets();
      blocks.push(
        <h4 key={`heading-${blocks.length}`} className="mt-3.5 text-[0.8125rem] font-semibold">
          {text.slice(4)}
        </h4>,
      );
    } else if (text.startsWith("- ")) {
      bullets.push(text.slice(2));
    } else {
      flushBullets();
      blocks.push(
        <p key={`text-${blocks.length}`} className="mt-1.5 text-[0.8125rem]">
          <Inline text={text} />
        </p>,
      );
    }
  }
  flushBullets();

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("whatsNew.title", { version })}</DialogTitle>
          <DialogDescription>{t("whatsNew.subtitle")}</DialogDescription>
        </DialogHeader>
        <div className="dense-scroll max-h-[55vh] overflow-y-auto pr-1">{blocks}</div>
        <DialogFooter>
          <Button onClick={onClose}>{t("whatsNew.gotIt")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
