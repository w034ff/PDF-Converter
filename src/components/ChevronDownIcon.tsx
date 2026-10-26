/**
 * The arrow of a button that opens a menu (design §10.1), drawn like the one
 * the WebView draws on a select box so that both read as the same control.
 * Decorative: the button carries its name in `aria-label`.
 */
export function ChevronDownIcon() {
  return (
    <svg
      width="10"
      height="7"
      viewBox="0 0 10 7"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M1.25 1.5 5 5.25 8.75 1.5" />
    </svg>
  );
}
