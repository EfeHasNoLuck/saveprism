import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { saveLocations, type SaveLocation } from "@/api/commands";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useNotify } from "./context";
import { describeError } from "./errors";

type LocationsState =
  | { status: "loading" }
  | { status: "ready"; locations: SaveLocation[] }
  | { status: "error"; message: string };

interface SaveLocationsDialogProps {
  onClose: () => void;
}

/**
 * "Where is my save?": the usual save folder of each engine, with an "Open folder" button for the
 * folders that exist on this PC. SavePrism never scans the disk; the host only checks the returned
 * candidates.
 */
export function SaveLocationsDialog({ onClose }: SaveLocationsDialogProps) {
  const { t } = useTranslation();
  const { notify } = useNotify();
  const [state, setState] = useState<LocationsState>({ status: "loading" });

  /** Opens a folder in Explorer; a refusal is reported as a toast. */
  function openFolder(folder: string) {
    if (folder === "") return;
    void openPath(folder).catch((error: unknown) => {
      const { reason, fix } = describeError(error);
      notify({
        kind: "error",
        title: t("locations.openFailed"),
        detail: t("toast.errorDetail", { reason, fix }).trim(),
      });
    });
  }

  useEffect(() => {
    let cancelled = false;
    saveLocations()
      .then((locations) => !cancelled && setState({ status: "ready", locations }))
      .catch((error: unknown) => {
        if (cancelled) return;
        const { reason, fix } = describeError(error);
        setState({
          status: "error",
          message: `${t("locations.failed")} ${t("toast.errorDetail", { reason, fix })}`.trim(),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [t]);

  return (
    <Dialog open onOpenChange={(open) => (open ? undefined : onClose())}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("locations.title")}</DialogTitle>
          <DialogDescription>{t("locations.description")}</DialogDescription>
        </DialogHeader>

        {state.status === "loading" ? (
          <p className="py-4 text-center text-[0.8125rem] text-muted-foreground">
            {t("common.loading")}
          </p>
        ) : state.status === "error" ? (
          <p role="alert" className="py-2 text-[0.8125rem] text-destructive">
            {state.message}
          </p>
        ) : (
          <div className="dense-scroll -mx-1 flex max-h-[60vh] flex-col gap-3 overflow-y-auto px-1">
            {state.locations.map((location) => (
              <section
                key={location.engine}
                className="flex flex-col gap-1 rounded-sm border border-border bg-card p-2.5"
              >
                <h3 className="text-[0.8125rem] font-semibold">
                  {t(`locations.engine.${location.engine}`)}
                </h3>
                <p className="text-[0.6875rem] text-muted-foreground">
                  {t(`locations.note.${location.engine}`)}
                </p>
                {location.paths.map((path) => (
                  <div key={path.shown} className="flex min-w-0 items-center gap-2">
                    <span
                      className="min-w-0 flex-1 truncate font-mono text-[0.6875rem] text-foreground-variant"
                      title={path.shown}
                    >
                      {path.shown}
                    </span>
                    {path.folder !== null ? (
                      <Button
                        variant="outline"
                        size="sm"
                        className="shrink-0"
                        onClick={() => {
                          openFolder(path.folder ?? "");
                        }}
                      >
                        <FolderOpen />
                        {t("locations.openFolder")}
                      </Button>
                    ) : (
                      <span className="shrink-0 text-[0.625rem] text-muted-foreground">
                        {t("locations.notFound")}
                      </span>
                    )}
                  </div>
                ))}
              </section>
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
