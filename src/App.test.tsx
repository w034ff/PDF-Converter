import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { App } from "./App";

describe("App", () => {
  afterEach(() => clearMocks());

  it("shows the app name", () => {
    mockIPC(() => new Promise(() => {}));
    render(<App />);
    expect(
      screen.getByRole("heading", { name: "PDF Converter" }),
    ).toBeInTheDocument();
  });

  it("shows that pdfium works", async () => {
    mockIPC((cmd) =>
      cmd === "get_about"
        ? {
            version: "0.1.0",
            pdfiumVersion: "chromium/8076",
            pdfiumReady: true,
            pdfiumError: null,
          }
        : undefined,
    );
    render(<App />);
    expect(await screen.findByTestId("pdfium-status")).toHaveTextContent(
      "pdfium chromium/8076: OK (app 0.1.0)",
    );
  });

  it("shows why pdfium does not work", async () => {
    mockIPC(() => ({
      version: "0.1.0",
      pdfiumVersion: "chromium/8076",
      pdfiumReady: false,
      pdfiumError: "PdfiumUnavailable: library not found",
    }));
    render(<App />);
    expect(await screen.findByTestId("pdfium-status")).toHaveTextContent(
      "PdfiumUnavailable: library not found",
    );
  });

  it("shows an unexpected answer as a failure", async () => {
    mockIPC(() => ({ version: 1 }));
    render(<App />);
    expect(await screen.findByTestId("pdfium-status")).toHaveTextContent(
      "unexpected value",
    );
  });

  it("switches tabs and updates aria-selected attributes", () => {
    mockIPC(() => new Promise(() => {}));
    render(<App initialNavLang="ja" />);

    const imagesTab = screen.getByRole("tab", { name: "画像 → PDF" });
    const pdfsTab = screen.getByRole("tab", { name: "PDF → 画像" });

    // Initially images tab is active (tabs use aria-selected, not aria-pressed)
    expect(imagesTab).toHaveAttribute("aria-selected", "true");
    expect(imagesTab).not.toHaveAttribute("aria-pressed");
    expect(pdfsTab).toHaveAttribute("aria-selected", "false");
    expect(pdfsTab).not.toHaveAttribute("aria-pressed");
    expect(
      screen.getByRole("button", { name: "PDF を保存" }),
    ).toBeInTheDocument();

    // Click PDF to Images tab
    fireEvent.click(pdfsTab);

    expect(imagesTab).toHaveAttribute("aria-selected", "false");
    expect(imagesTab).not.toHaveAttribute("aria-pressed");
    expect(pdfsTab).toHaveAttribute("aria-selected", "true");
    expect(pdfsTab).not.toHaveAttribute("aria-pressed");
    expect(
      screen.getByRole("button", { name: "変換を開始" }),
    ).toBeInTheDocument();
  });

  it("switches UI language on language select change", () => {
    mockIPC(() => new Promise(() => {}));
    render(<App initialNavLang="ja" />);

    const select = screen.getByRole("combobox");
    expect(select).toHaveValue("ja");
    expect(screen.getByRole("tab", { name: "画像 → PDF" })).toBeInTheDocument();

    // Switch to English
    fireEvent.change(select, { target: { value: "en" } });

    expect(select).toHaveValue("en");
    expect(
      screen.getByRole("tab", { name: "Images → PDF" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "PDF → Images" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save PDF" }),
    ).toBeInTheDocument();
  });

  it("initializes with English when initialNavLang is en-US", () => {
    mockIPC(() => new Promise(() => {}));
    render(<App initialNavLang="en-US" />);

    expect(
      screen.getByRole("tab", { name: "Images → PDF" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Save PDF" }),
    ).toBeInTheDocument();
  });
});
