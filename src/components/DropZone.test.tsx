import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DropZone } from "./DropZone";

describe("DropZone", () => {
  it("renders title and description", () => {
    render(
      <DropZone
        title="画像をここにドロップ"
        description="PNG、JPEG、WebP、BMP に対応。"
      />,
    );

    expect(screen.getByText("画像をここにドロップ")).toBeInTheDocument();
    expect(
      screen.getByText("PNG、JPEG、WebP、BMP に対応。"),
    ).toBeInTheDocument();
  });

  it("renders action buttons and handles click events", () => {
    const handleAddFiles = vi.fn();
    const handleAddFolder = vi.fn();

    render(
      <DropZone
        title="Drop files"
        description="Description"
        addFilesLabel="Add Images"
        addFolderLabel="Add Folder"
        onAddFiles={handleAddFiles}
        onAddFolder={handleAddFolder}
      />,
    );

    const filesBtn = screen.getByRole("button", { name: "Add Images" });
    const folderBtn = screen.getByRole("button", { name: "Add Folder" });

    fireEvent.click(filesBtn);
    expect(handleAddFiles).toHaveBeenCalledTimes(1);

    fireEvent.click(folderBtn);
    expect(handleAddFolder).toHaveBeenCalledTimes(1);
  });

  it("applies is-drag-over class when isDragOver is true", () => {
    const { rerender } = render(
      <DropZone title="Title" description="Description" isDragOver={false} />,
    );

    const dropZone = screen.getByTestId("drop-zone");
    expect(dropZone).not.toHaveClass("is-drag-over");

    rerender(
      <DropZone title="Title" description="Description" isDragOver={true} />,
    );

    expect(dropZone).toHaveClass("is-drag-over");
  });

  it("renders custom icon if provided", () => {
    render(
      <DropZone
        title="Title"
        description="Description"
        icon={<span data-testid="custom-icon">Custom Icon</span>}
      />,
    );

    expect(screen.getByTestId("custom-icon")).toBeInTheDocument();
  });
});
