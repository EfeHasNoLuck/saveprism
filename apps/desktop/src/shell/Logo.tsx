/**
 * The SavePrism mark: two squares (one turned 45 degrees) that interlock into an 8-point star.
 * Blues are the brand colours; four short pieces of the light square are drawn over the dark one
 * so the squares look woven.
 */
export function Logo({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      className={className}
      fill="none"
      strokeWidth="1.8"
      strokeLinejoin="round"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M12 2.1L21.9 12L12 21.9L2.1 12Z" stroke="#8FC0F7" />
      <path d="M5 5H19V19H5Z" stroke="#4A7FD6" />
      <path
        d="M17.8 7.9L20.2 10.3M16.1 17.8L13.7 20.2M6.2 16.1L3.8 13.7M7.9 6.2L10.3 3.8"
        stroke="#8FC0F7"
      />
    </svg>
  );
}
