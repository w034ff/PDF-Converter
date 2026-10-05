import { ToggleButton } from "./ToggleButton";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
}

export interface SegmentedControlProps<T extends string> {
  label: string;
  options: readonly SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
}

/**
 * Segmented control group that renders a group of ToggleButtons
 * with role="group" and an accessible aria-label (design §10.1).
 */
export function SegmentedControl<T extends string>({
  label,
  options,
  value,
  onChange,
  disabled,
  className = "seg",
}: SegmentedControlProps<T>) {
  return (
    <div role="group" aria-label={label} className={className}>
      {options.map((option) => (
        <ToggleButton
          key={option.value}
          pressed={option.value === value}
          disabled={disabled}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </ToggleButton>
      ))}
    </div>
  );
}
