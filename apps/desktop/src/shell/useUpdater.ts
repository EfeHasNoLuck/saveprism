import { useCallback, useRef, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import {
  nextProgress,
  progressPercent,
  runUpdateCheck,
  startInstall,
  type InstallProgress,
  type UpdateCheck,
} from "./update";

/** A check that hangs (a captive portal, a dead proxy) gives up after this many milliseconds. */
const CHECK_TIMEOUT_MS = 30_000;

export interface Updater {
  /** What Settings → About shows. */
  check: UpdateCheck;
  /** Asks GitHub for a newer version. A quiet check changes nothing on screen unless one exists. */
  runCheck: (quiet: boolean) => Promise<UpdateCheck>;
  /** The download and install under way, else null. */
  progress: InstallProgress | null;
  /** Downloads and installs the found update, then restarts the app. False when it failed. */
  install: () => Promise<boolean>;
}

/** The version check and the update of the running app (Tauri's updater plugin). */
export function useUpdater(): Updater {
  const [checkState, setCheckState] = useState<UpdateCheck>({ status: "idle" });
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const updateRef = useRef<Update | null>(null);
  const installing = useRef(false);

  const runCheck = useCallback(async (quiet: boolean) => {
    if (!quiet) setCheckState({ status: "checking" });
    const outcome = await runUpdateCheck(() => check({ timeout: CHECK_TIMEOUT_MS }), quiet);
    if (outcome.update !== null && !installing.current) {
      // The host keeps one resource per answer; let go of the older one.
      updateRef.current?.close().catch(() => undefined);
      updateRef.current = outcome.update;
    }
    if (!quiet || outcome.check.status === "available") setCheckState(outcome.check);
    return outcome.check;
  }, []);

  const install = useCallback(async () => {
    const update = updateRef.current;
    if (update === null || installing.current) return false;
    installing.current = true;
    let current = startInstall(update.version);
    setProgress(current);
    try {
      await update.downloadAndInstall((event) => {
        const next = nextProgress(current, event);
        // One render per whole percent is enough for the bar.
        const changed =
          next.phase !== current.phase || progressPercent(next) !== progressPercent(current);
        current = next;
        if (changed) setProgress(next);
      });
      // On Windows the installer has closed the app by now and starts it again by itself.
      setProgress({ ...current, phase: "install" });
      await relaunch();
      return true;
    } catch {
      installing.current = false;
      setProgress(null);
      return false;
    }
  }, []);

  return { check: checkState, runCheck, progress, install };
}
