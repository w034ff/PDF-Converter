import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ToggleButton } from "./ToggleButton";

describe("ToggleButton", () => {
  it('renders with aria-pressed="true" when pressed is true', () => {
    render(<ToggleButton pressed={true}>Option</ToggleButton>);
    const button = screen.getByRole("button", { name: "Option" });
    expect(button).toHaveAttribute("aria-pressed", "true");
  });

  it('renders with aria-pressed="false" when pressed is false', () => {
    render(<ToggleButton pressed={false}>Option</ToggleButton>);
    const button = screen.getByRole("button", { name: "Option" });
    expect(button).toHaveAttribute("aria-pressed", "false");
  });

  it("calls onClick and onPressedChange when clicked", () => {
    const handleClick = vi.fn();
    const handlePressedChange = vi.fn();

    render(
      <ToggleButton
        pressed={false}
        onClick={handleClick}
        onPressedChange={handlePressedChange}
      >
        Click Me
      </ToggleButton>,
    );

    const button = screen.getByRole("button", { name: "Click Me" });
    fireEvent.click(button);

    expect(handleClick).toHaveBeenCalledTimes(1);
    expect(handlePressedChange).toHaveBeenCalledWith(true);
  });

  it("does not fire events when disabled", () => {
    const handleClick = vi.fn();
    const handlePressedChange = vi.fn();

    render(
      <ToggleButton
        pressed={false}
        disabled={true}
        onClick={handleClick}
        onPressedChange={handlePressedChange}
      >
        Disabled
      </ToggleButton>,
    );

    const button = screen.getByRole("button", { name: "Disabled" });
    expect(button).toBeDisabled();
    fireEvent.click(button);

    expect(handleClick).not.toHaveBeenCalled();
    expect(handlePressedChange).not.toHaveBeenCalled();
  });
});
