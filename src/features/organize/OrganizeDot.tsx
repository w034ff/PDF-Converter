export interface OrganizeDotProps {
  /** `OrganizePdfEntry.colorIndex`, from 0; the color variables count from 1. */
  colorIndex: number;
}

/** The dot in the color of a PDF (organize §4). */
export function OrganizeDot({ colorIndex }: OrganizeDotProps) {
  return (
    <span
      className="organize-dot"
      style={{ background: `var(--organize-source-${colorIndex + 1})` }}
    />
  );
}
