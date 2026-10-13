export interface LockIconProps {
  size: number;
  className?: string;
}

/**
 * The key mark of a PDF waiting for its password (design §10.1). Decorative:
 * the text beside it carries the meaning. The stroke comes from CSS.
 */
export function LockIcon({ size, className }: LockIconProps) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </svg>
  );
}
