import type { ButtonHTMLAttributes, ReactNode } from "react";

export interface ToggleButtonProps extends Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "type" | "aria-pressed"
> {
  pressed: boolean;
  children: ReactNode;
  onPressedChange?: (pressed: boolean) => void;
}

/**
 * Toggle button that exposes its pressed state via aria-pressed.
 * Used individually or inside a segmented control group.
 */
export function ToggleButton({
  pressed,
  children,
  onPressedChange,
  onClick,
  disabled,
  className,
  ...rest
}: ToggleButtonProps) {
  function handleClick(e: React.MouseEvent<HTMLButtonElement>) {
    if (disabled) {
      return;
    }
    onClick?.(e);
    onPressedChange?.(!pressed);
  }

  return (
    <button
      type="button"
      aria-pressed={pressed}
      disabled={disabled}
      onClick={handleClick}
      className={className}
      {...rest}
    >
      {children}
    </button>
  );
}
