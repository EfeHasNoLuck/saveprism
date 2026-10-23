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
 * How many entry columns fit in the element behind `ref`. The count follows the element's width,
 * so a wide card shows more columns and a page holds more entries.
 */
export function useGridColumns<T extends HTMLElement = HTMLDivElement>() {
  const { ref, width } = useElementWidth<T>();
  return { ref, columns: columnsIn(width) };
}
