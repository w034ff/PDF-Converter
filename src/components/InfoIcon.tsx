export interface InfoIconProps {
  size: number;
  className?: string;
}

/**
 * The "i" mark of a notice that is not about a password (design §10.1).
 * Decorative: the text beside it carries the meaning. The stroke comes from
 * CSS.
 */
export function InfoIcon({ size, className }: InfoIconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      strokeWidth="2"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5" />
      <path d="M12 8h.01" />
    </svg>
  );
}
