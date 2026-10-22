import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useNotify } from "./context";

/** Toasts at the bottom right, above the bottom bar. Errors are announced to screen readers. */
export function Toasts() {
  const { t } = useTranslation();
  const { toasts, dismissToast } = useNotify();
  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed right-4 bottom-20 z-[60] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          role={toast.kind === "error" ? "alert" : "status"}
          className={cn(
            "pointer-events-auto flex flex-col gap-1 rounded-xl border bg-popover p-4 text-popover-foreground shadow-lg",
            toast.kind === "success" && "border-success/60",
            toast.kind === "error" && "border-destructive/60",
            toast.kind === "info" && "border-border",
          )}
        >
          <div className="flex items-start justify-between gap-2">
            <p
              className={cn(
                "text-sm font-semibold",
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
              className="rounded-md p-0.5 text-muted-foreground outline-none hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              <X className="size-4" />
            </button>
          </div>
          {toast.detail ? (
            <p className="text-xs break-words text-muted-foreground">{toast.detail}</p>
          ) : null}
          {toast.action ? (
            <Button
              variant="outline"
              size="sm"
              className="mt-1"
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
