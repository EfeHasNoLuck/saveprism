/** Fades out the opening cover painted by `index.html`, then removes it from the page. */

const COVER_ID = "boot-cover";
const FADE_MS = 150;

function prefersReducedMotion(): boolean {
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    return false;
  }
}

/**
 * Called once React has mounted. The cover never waits for its own animation: it starts fading
 * as soon as the app is on screen, and it has `pointer-events: none` the whole time.
 */
export function dismissBootCover(): void {
  const cover = document.getElementById(COVER_ID);
  if (!cover || cover.classList.contains("is-done")) return;
  if (prefersReducedMotion()) {
    cover.remove();
    return;
  }
  cover.classList.add("is-done");
  window.setTimeout(() => cover.remove(), FADE_MS);
}
