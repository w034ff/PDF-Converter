import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { NoticeDisplay } from "./NoticeDisplay";

describe("NoticeDisplay", () => {
  it('renders the message with role="status", not as an alert', () => {
    render(<NoticeDisplay message="パスワードで保護されています" />);

    expect(screen.getByRole("status")).toHaveTextContent(
      "パスワードで保護されています",
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("renders the action and no dismiss button", () => {
    render(
      <NoticeDisplay
        message="パスワードで保護されています"
        action={<button type="button">パスワードを入力</button>}
      />,
    );

    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(
      screen.getByRole("button", { name: "パスワードを入力" }),
    ).toBeInTheDocument();
  });

  it("shows the lock unless asked for the info mark", () => {
    const { container, rerender } = render(<NoticeDisplay message="a" />);
    expect(container.querySelector("rect")).not.toBeNull();
    expect(container.querySelector("circle")).toBeNull();

    rerender(<NoticeDisplay message="a" icon="info" />);
    expect(container.querySelector("rect")).toBeNull();
    expect(container.querySelector("circle")).not.toBeNull();
  });
});
