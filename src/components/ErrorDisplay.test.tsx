import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorDisplay } from "./ErrorDisplay";

describe("ErrorDisplay", () => {
  it('renders with role="alert" and displays message', () => {
    render(<ErrorDisplay message="画像の読み込みに失敗しました" />);

    const alert = screen.getByRole("alert");
    expect(alert).toBeInTheDocument();
    expect(
      screen.getByText("画像の読み込みに失敗しました"),
    ).toBeInTheDocument();
  });

  it("renders detail string when provided", () => {
    render(
      <ErrorDisplay
        message="画像が大きすぎます"
        detail="上限 80,000,000 ピクセル"
      />,
    );

    expect(screen.getByText("画像が大きすぎます")).toBeInTheDocument();
    expect(screen.getByText("上限 80,000,000 ピクセル")).toBeInTheDocument();
  });

  it("calls onDismiss when dismiss button is clicked", () => {
    const handleDismiss = vi.fn();
    render(
      <ErrorDisplay
        message="Error"
        onDismiss={handleDismiss}
        dismissLabel="閉じる"
      />,
    );

    const button = screen.getByRole("button", { name: "閉じる" });
    fireEvent.click(button);
    expect(handleDismiss).toHaveBeenCalledTimes(1);
  });
});
