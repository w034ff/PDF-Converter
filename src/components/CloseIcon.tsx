export interface CloseIconProps {
  size: number;
}

/**
 * The "×" of a dialog's close button (design §10.1). Decorative: the button
 * carries the name in `aria-label`.
 */
export function CloseIcon({ size }: CloseIconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <line x1="18" y1="6" x2="6" y2="18" />
      <line x1="6" y1="6" x2="18" y2="18" />
    </svg>
  );
}
