import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useNotify } from "./context";
import { PathText } from "./PathText";

/** Toasts at the bottom right, above the bottom bar. Errors are announced to screen readers. */
export function Toasts() {
  const { t } = useTranslation();
  const { toasts, dismissToast } = useNotify();
  if (toasts.length === 0) return null;

  return (
    <div
      data-toasts
      className="pointer-events-none fixed right-3 bottom-13 z-[60] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.kind === "error" ? "alert" : "status"}
          className={cn(
            "pointer-events-auto relative flex flex-col gap-1 overflow-hidden rounded-md border border-border bg-popover py-2.5 pr-2.5 pl-4 text-popover-foreground shadow-lg shadow-black/20",
          )}
        >
          <span
            aria-hidden="true"
            className={cn(
              "absolute inset-y-0 left-0 w-[3px]",
              toast.kind === "success" && "bg-success",
              toast.kind === "error" && "bg-destructive",
              toast.kind === "info" && "bg-primary",
            )}
          />
          <div className="flex items-start justify-between gap-2">
            <p
              className={cn(
                "text-[0.8125rem] font-semibold",
                toast.kind === "success" && "text-success",
                toast.kind === "error" && "text-destructive",
              )}
            >
              {toast.title}
            </p>
            <button
              type="button"
              aria-label={t("toast.dismiss")}
              onClick={() => dismissToast(toast.id)}
              className="rounded-sm p-0.5 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <X className="size-4" />
            </button>
          </div>
          {toast.detail ? (
            <p className="text-xs break-words text-muted-foreground">
              <PathText text={toast.detail} />
            </p>
          ) : null}
          {toast.action ? (
            <Button
              variant="outline"
              size="sm"
              className="mt-1 self-start"
              onClick={() => {
                toast.action?.run();
              }}
            >
              {toast.action.label}
            </Button>
          ) : null}
        </div>
      ))}
    </div>
  );
}
