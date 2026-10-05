import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

describe("SegmentedControl", () => {
  const options = [
    { value: "option1", label: "First" },
    { value: "option2", label: "Second" },
  ] as const;

  it("renders a group with accessible aria-label and options", () => {
    render(
      <SegmentedControl
        label="Test Group"
        options={options}
        value="option1"
        onChange={vi.fn()}
      />,
    );

    const group = screen.getByRole("group", { name: "Test Group" });
    expect(group).toBeInTheDocument();

    const first = screen.getByRole("button", { name: "First" });
    const second = screen.getByRole("button", { name: "Second" });

    expect(first).toHaveAttribute("aria-pressed", "true");
    expect(second).toHaveAttribute("aria-pressed", "false");
  });

  it("calls onChange when another option is clicked", () => {
    const handleChange = vi.fn();
    render(
      <SegmentedControl
        label="Test Group"
        options={options}
        value="option1"
        onChange={handleChange}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Second" }));
    expect(handleChange).toHaveBeenCalledWith("option2");
  });

  it("disables all buttons when disabled prop is true", () => {
    render(
      <SegmentedControl
        label="Test Group"
        options={options}
        value="option1"
        onChange={vi.fn()}
        disabled={true}
      />,
    );

    expect(screen.getByRole("button", { name: "First" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Second" })).toBeDisabled();
  });
});
