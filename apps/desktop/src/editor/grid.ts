import { useLayoutEffect, useRef, useState } from "react";
import { columnsIn } from "./helpers";

/** The content width of the element behind `ref`, kept up to date while it resizes. */
export function useElementWidth<T extends HTMLElement = HTMLDivElement>() {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) return;
    // The observer reports the first size as soon as it starts watching.
    const observer = new ResizeObserver(() => setWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/**
 * The root font size as a share of the default 16px (the Text size setting); 1 when unknown. The
 * column helpers count in pixels at 16px, so a measured width is divided by this first.
 */
export function rootScale(): number {
  const size = Number.parseFloat(getComputedStyle(document.documentElement).fontSize);
  return Number.isFinite(size) && size > 0 ? size / 16 : 1;
}

/**
 * How many entry columns fit in the element behind `ref`. The count follows the element's width,
 * so a wide card shows more columns and a page holds more entries.
 */
export function useGridColumns<T extends HTMLElement = HTMLDivElement>() {
  const { ref, width } = useElementWidth<T>();
  return { ref, columns: columnsIn(width / rootScale()) };
}
