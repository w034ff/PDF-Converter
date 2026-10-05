import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressBar } from "./ProgressBar";

describe("ProgressBar", () => {
  it("renders progressbar with correct ARIA attributes", () => {
    render(<ProgressBar value={3} max={6} label="進捗" />);

    const bar = screen.getByRole("progressbar", { name: "進捗" });
    expect(bar).toHaveAttribute("aria-valuenow", "3");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "6");
  });

  it("calculates width percentage correctly", () => {
    const { rerender } = render(<ProgressBar value={2} max={4} />);
    const fill = screen.getByTestId("progress-bar-fill");
    expect(fill).toHaveStyle({ width: "50%" });

    rerender(<ProgressBar value={0} max={10} />);
    expect(fill).toHaveStyle({ width: "0%" });

    rerender(<ProgressBar value={10} max={10} />);
    expect(fill).toHaveStyle({ width: "100%" });
  });

  it("clamps width within 0% and 100%", () => {
    const { rerender } = render(<ProgressBar value={-5} max={10} />);
    const fill = screen.getByTestId("progress-bar-fill");
    expect(fill).toHaveStyle({ width: "0%" });

    rerender(<ProgressBar value={15} max={10} />);
    expect(fill).toHaveStyle({ width: "100%" });
  });

  it("renders status text and counter when provided", () => {
    render(
      <ProgressBar
        value={3}
        max={6}
        statusText="変換中：page-04.png ほか 1 件"
        showCount={true}
      />,
    );

    expect(
      screen.getByText("変換中：page-04.png ほか 1 件"),
    ).toBeInTheDocument();
    expect(screen.getByText("3 / 6")).toBeInTheDocument();
  });
});
