import { useId } from "react";

/** The outline of the SavePrism "S": a hexagonal prism folded into the letter. */
const S_PATH =
  "M56 0 100 25V50L74 65V39L56 29 28 45 100 85V110L44 142 0 117V92L26 77V103L44 113 72 97 0 57V32Z";

/**
 * The SavePrism mark (from `docs/brand/SavePrism-icon-transparent.svg`): a light-blue "S" with
 * two blue faces and one cyan face clipped to its outline. The view box is square so the mark
 * sizes like an icon.
 */
export function Logo({ className }: { className?: string }) {
  const clip = useId();
  return (
    <svg viewBox="-21 0 142 142" className={className} aria-hidden="true">
      <defs>
        <clipPath id={clip}>
          <path d={S_PATH} />
        </clipPath>
      </defs>
      <path d={S_PATH} fill="#ABC7FF" />
      <g clipPath={`url(#${clip})`}>
        <path d="M56 29 82 14 100 25 74 40Z" fill="#5D90E8" />
        <path d="M74 39 100 25V50L74 65Z" fill="#4CD7F6" />
        <path d="M0 92 26 77V103L0 117Z" fill="#5D90E8" />
      </g>
    </svg>
  );
}
