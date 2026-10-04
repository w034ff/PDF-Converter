import { render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
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
});
