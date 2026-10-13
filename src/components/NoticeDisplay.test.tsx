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
});
