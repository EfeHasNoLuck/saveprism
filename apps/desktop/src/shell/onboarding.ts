/** Storage keys and the pure rules of the first-launch tour and the "What's new" dialog. */

import { browserStorage, type ReadableStorage, type WritableStorage } from "./settings";

/** The app version whose notes the player has already seen. */
export const LAST_SEEN_VERSION_KEY = "saveprism.lastSeenVersion";
/** Set once the first-launch tour has run. */
export const TOUR_SEEN_KEY = "saveprism.tourSeen";

export interface WhatsNewDecision {
  /** Show the dialog now. */
  show: boolean;
  /** Store this version now (first launch); null means leave the stored value alone. */
  store: string | null;
}

/**
 * First launch (nothing stored) shows the tour, not this dialog, and stores the version.
 * Same version shows nothing. A different stored version means the app was updated: show it.
 */
export function whatsNewDecision(stored: string | null, running: string): WhatsNewDecision {
  const version = running.trim();
  if (version === "") return { show: false, store: null };
  if (stored === null || stored.trim() === "") return { show: false, store: version };
  if (stored === version) return { show: false, store: null };
  return { show: true, store: null };
}

/** Reads a storage key, treating a broken storage as empty. */
export function readStored(key: string, storage: ReadableStorage | null = browserStorage()): string | null {
  try {
    return storage ? storage.getItem(key) : null;
  } catch {
    return null;
  }
}

/** Writes a storage key; a broken storage just keeps the choice for this session. */
export function writeStored(
  key: string,
  value: string,
  storage: WritableStorage | null = browserStorage(),
): void {
  try {
    storage?.setItem(key, value);
  } catch {
    // Storage unavailable: nothing else to do.
  }
}

/** True when the first-launch tour has already run on this PC. */
export function tourSeen(storage: ReadableStorage | null = browserStorage()): boolean {
  return readStored(TOUR_SEEN_KEY, storage) === "1";
}
