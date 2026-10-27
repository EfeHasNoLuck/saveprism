import { useEffect } from "react";
import { dismissBootCover } from "./boot";

/** Takes the opening cover away as soon as React has committed the first render. */
export function BootGate() {
  useEffect(() => {
    dismissBootCover();
  }, []);
  return null;
}
