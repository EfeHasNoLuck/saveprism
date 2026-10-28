/**
 * Pure update logic (no React, no Tauri). The check and the download are passed in, so every step
 * can be tested with fakes. The only request goes to the GitHub release of SavePrism.
 */

/** How long after the window is ready the start check runs, in milliseconds. */
export const START_CHECK_DELAY_MS = 4000;

/** What Settings → About shows for the version check. */
export type UpdateCheck =
  | { status: "idle" }
  | { status: "checking" }
  | { status: "current" }
  | { status: "available"; version: string }
  | { status: "failed" };

/** The part of the updater's answer this logic needs: null when this is the newest version. */
export interface FoundUpdate {
  version: string;
}

export interface CheckOutcome<T extends FoundUpdate> {
  check: UpdateCheck;
  /** The update to install, when a newer version exists. */
  update: T | null;
}

/**
 * Runs one version check. A quiet check (the one at start) never reports "up to date" or an
 * error: both leave About as it was, so nothing shows unless a newer version exists.
 */
export async function runUpdateCheck<T extends FoundUpdate>(
  check: () => Promise<T | null>,
  quiet: boolean,
): Promise<CheckOutcome<T>> {
  let update: T | null;
  try {
    update = await check();
  } catch {
    return { check: quiet ? { status: "idle" } : { status: "failed" }, update: null };
  }
  if (update === null) {
    return { check: quiet ? { status: "idle" } : { status: "current" }, update: null };
  }
  return { check: { status: "available", version: update.version }, update };
}

/** The download events of the updater plugin. */
export type DownloadEventLike =
  | { event: "Started"; data: { contentLength?: number } }
  | { event: "Progress"; data: { chunkLength: number } }
  | { event: "Finished" };

/** Where an update stands while it downloads and installs. */
export interface InstallProgress {
  /** The version being installed. */
  version: string;
  /** "download" while bytes arrive; "install" once the installer starts (the app then restarts). */
  phase: "download" | "install";
  downloaded: number;
  /** Total size in bytes, or null when the server did not say. */
  total: number | null;
}

export function startInstall(version: string): InstallProgress {
  return { version, phase: "download", downloaded: 0, total: null };
}

/** The progress after one download event. */
export function nextProgress(progress: InstallProgress, event: DownloadEventLike): InstallProgress {
  switch (event.event) {
    case "Started": {
      const length = event.data.contentLength;
      const total = typeof length === "number" && Number.isFinite(length) && length > 0 ? length : null;
      return { ...progress, phase: "download", downloaded: 0, total };
    }
    case "Progress":
      return { ...progress, downloaded: progress.downloaded + Math.max(0, event.data.chunkLength) };
    case "Finished":
      return { ...progress, phase: "install" };
  }
}

/** Whole percent downloaded (0 to 100), or null when the total size is unknown. */
export function progressPercent(progress: InstallProgress): number | null {
  if (progress.total === null) return null;
  return Math.min(100, Math.max(0, Math.floor((progress.downloaded / progress.total) * 100)));
}
