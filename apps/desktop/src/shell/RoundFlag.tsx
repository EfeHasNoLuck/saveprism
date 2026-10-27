/** Small round flag icons for the language choices (Windows cannot draw flag emoji). */

import type { Language } from "@/i18n";

/** The two flags are drawn here, so no image files or font glyphs are needed. */
export function RoundFlag({ language, className }: { language: Language; className?: string }) {
  return (
    <span
      className={`inline-flex size-4 shrink-0 overflow-hidden rounded-full ring-1 ring-black/20 ${className ?? ""}`}
      aria-hidden="true"
    >
      {language === "tr" ? <TurkishFlag /> : <UkFlag />}
    </span>
  );
}

function UkFlag() {
  return (
    <svg viewBox="0 0 32 32" className="block size-full">
      <rect width="32" height="32" fill="#012169" />
      <path d="M0 0L32 32M32 0L0 32" stroke="#ffffff" strokeWidth="7" />
      <path d="M0 0L32 32M32 0L0 32" stroke="#c8102e" strokeWidth="3" />
      <path d="M16 0V32M0 16H32" stroke="#ffffff" strokeWidth="10" />
      <path d="M16 0V32M0 16H32" stroke="#c8102e" strokeWidth="6" />
    </svg>
  );
}

function TurkishFlag() {
  return (
    <svg viewBox="0 0 32 32" className="block size-full">
      <rect width="32" height="32" fill="#e30a17" />
      <circle cx="13" cy="16" r="7" fill="#ffffff" />
      <circle cx="15.2" cy="16" r="5.6" fill="#e30a17" />
      <polygon
        points="23,11.8 24.2,15 27.5,15 24.9,17 25.8,20.2 23,18.2 20.2,20.2 21.1,17 18.5,15 21.8,15"
        fill="#ffffff"
      />
    </svg>
  );
}
