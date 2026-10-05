import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ListRow } from "./ListRow";

describe("ListRow", () => {
  it("renders index, title, and metadata", () => {
    render(
      <ListRow
        index={1}
        title="photo.jpg"
        meta="2480 × 3508 · JPEG · 1.2 MB"
      />,
    );

    expect(screen.getByText("1")).toBeInTheDocument();
    expect(screen.getByText("photo.jpg")).toBeInTheDocument();
    expect(screen.getByText("2480 × 3508 · JPEG · 1.2 MB")).toBeInTheDocument();
  });

  it("renders thumbnail image and accessible label", () => {
    render(
      <ListRow
        title="diagram.png"
        thumbnailSrc="blob:test-thumbnail"
        thumbnailAlt="diagram.png のサムネイル"
      />,
    );

    const img = screen.getByRole("img", { name: "diagram.png のサムネイル" });
    expect(img).toBeInTheDocument();
  });

  it("renders error message when error prop is provided", () => {
    render(<ListRow title="corrupt.png" error="画像を読み込めませんでした" />);

    expect(screen.getByText("画像を読み込めませんでした")).toBeInTheDocument();
  });

  it("renders status when status prop is provided", () => {
    render(
      <ListRow title="page-01.png" status={{ text: "✓ 完了", kind: "ok" }} />,
    );

    expect(screen.getByText("✓ 完了")).toBeInTheDocument();
  });

  it("fires callbacks on action button clicks", () => {
    const handleMoveUp = vi.fn();
    const handleMoveDown = vi.fn();
    const handleRemove = vi.fn();

    render(
      <ListRow
        title="test.png"
        onMoveUp={handleMoveUp}
        onMoveDown={handleMoveDown}
        onRemove={handleRemove}
        labels={{
          moveUp: "Move Up",
          moveDown: "Move Down",
          remove: "Remove",
        }}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Move Up" }));
    expect(handleMoveUp).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Move Down" }));
    expect(handleMoveDown).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Remove" }));
    expect(handleRemove).toHaveBeenCalledTimes(1);
  });

  it("disables move buttons when canMoveUp or canMoveDown is false", () => {
    render(
      <ListRow
        title="test.png"
        onMoveUp={vi.fn()}
        onMoveDown={vi.fn()}
        canMoveUp={false}
        canMoveDown={false}
        labels={{
          moveUp: "Up",
          moveDown: "Down",
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "Up" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Down" })).toBeDisabled();
  });
});
