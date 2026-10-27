import { useId } from "react";
import type { Language } from "./detect";
import { LANGUAGE_FLAG } from "./language";

interface FlagProps {
  className?: string;
}

/** The round UK flag used for English. */
export function UkFlag({ className }: FlagProps) {
  const clip = `uk-${useId().replace(/[^a-zA-Z0-9_-]/g, "")}`;
  return (
    <svg viewBox="0 0 60 60" className={className} aria-hidden="true">
      <defs>
        <clipPath id={clip}>
          <circle cx="30" cy="30" r="30" />
        </clipPath>
      </defs>
      <g clipPath={`url(#${clip})`}>
        <rect width="60" height="60" fill="#012169" />
        <path d="M0 0L60 60M60 0L0 60" stroke="#fff" strokeWidth="12" />
        <path d="M0 0L60 60M60 0L0 60" stroke="#C8102E" strokeWidth="4" />
        <path d="M30 0V60M0 30H60" stroke="#fff" strokeWidth="18" />
        <path d="M30 0V60M0 30H60" stroke="#C8102E" strokeWidth="10" />
      </g>
    </svg>
  );
}

const STAR_POINTS = (() => {
  const points: string[] = [];
  for (let k = 0; k < 10; k += 1) {
    const angle = Math.PI + (k * Math.PI) / 5;
    const radius = k % 2 === 0 ? 5.6 : 2.25;
    points.push(`${(41 + radius * Math.cos(angle)).toFixed(2)},${(30 + radius * Math.sin(angle)).toFixed(2)}`);
  }
  return points.join(" ");
})();

/** The round flag of Turkey. */
export function TurkeyFlag({ className }: FlagProps) {
  return (
    <svg viewBox="0 0 60 60" className={className} aria-hidden="true">
      <circle cx="30" cy="30" r="30" fill="#E30A17" />
      <circle cx="25" cy="30" r="11.5" fill="#fff" />
      <circle cx="28" cy="30" r="9.2" fill="#E30A17" />
      <polygon points={STAR_POINTS} fill="#fff" />
    </svg>
  );
}

/** The flag of a shipped language. */
export function LanguageFlag({ language, className }: FlagProps & { language: Language }) {
  return LANGUAGE_FLAG[language] === "tr" ? (
    <TurkeyFlag className={className} />
  ) : (
    <UkFlag className={className} />
  );
}
