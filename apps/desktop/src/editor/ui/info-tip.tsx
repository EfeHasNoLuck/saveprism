import { Info } from "lucide-react";
import { Tooltip } from "radix-ui";
import { useTranslation } from "react-i18next";

/** Small (i) icon that shows `text` in a tooltip. Needs a `Tooltip.Provider` above it. */
export function InfoTip({ text }: { text: string }) {
  const { t } = useTranslation("editor");
  return (
    <Tooltip.Root>
      <Tooltip.Trigger asChild>
        <button
          type="button"
          aria-label={t("field.info")}
          className="inline-flex size-4 items-center justify-center rounded-full text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
        >
          <Info className="size-3.5" />
        </button>
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Content
          side="top"
          sideOffset={6}
          className="z-50 max-w-64 rounded-md bg-foreground px-2.5 py-1.5 text-xs text-background"
        >
          {text}
        </Tooltip.Content>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}
