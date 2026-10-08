# PDF Converter

[日本語 (Japanese)](README.ja.md)

PDF Converter is an open-source desktop application that converts raster images (PNG, JPEG, WebP, BMP) to PDF and PDF files to raster images (PNG, JPEG) locally on your machine.
All processing runs entirely on your computer without uploading any files to external online services. Supported operating systems are Windows and Linux (x64).

## Features

- **Images → PDF**: Converts images to PDF without compromising original quality. JPEG images are embedded directly without recompression, and transparent PNG, WebP, and BMP images retain transparency with lossless compression. Respects EXIF orientation and supports both "Fit to image" and "A4" page sizes.
- **Merge & Reorder**: Combine multiple images into a single multi-page PDF. Flexible reordering via drag-and-drop or keyboard shortcuts (↑ / ↓, Alt + ↑ / ↓). Individual items can also be removed from the list.
- **PDF → Images**: Render PDF pages into PNG or JPEG images. Select from 72 dpi (screen), 150 dpi (standard), or 300 dpi (print) resolutions, rendered with crisp white backgrounds.
- **Page Range Selection**: Convert all pages or specify custom page ranges (e.g. `1-3, 5`). Select individual pages interactively by clicking thumbnails for single PDFs, or apply the same range across multiple PDFs in bulk.
- **Batch Conversion & Cancellation**: Convert multiple files in bulk to a chosen destination folder utilizing multi-core parallel processing. Can be cancelled at any time without leaving incomplete or corrupted files in the output directory.
- **Sequential Numbering & No Overwriting**: Automatically resolves output name conflicts by appending sequential numbers (such as `name (1).pdf` or `name_p1 (1).png`) to ensure existing files are never overwritten.
- **Bilingual Interface**: Full interface support for Japanese and English. Your preferred language setting is remembered across restarts.
- **Settings Persistence**: Remembers your last-used conversion settings (page size, output format, resolution, output folder, etc.) and restores them automatically on next launch.
- **Password-Protected PDFs**: Password-protected PDF files cannot be opened.
- **No Network Communication**: The application itself makes no network requests (contains no telemetry and no update checks). On Windows, the Microsoft Edge WebView2 runtime used to display the UI connects to a Microsoft service (`substrate.office.com`) on startup on its own; the application cannot disable this.

## Installation

Download the appropriate installer or package for your operating system from the [Releases](https://github.com/w034ff/PDF-Converter/releases) page.

### Windows

- **Formats**: The NSIS installer (`.exe`) is recommended for individual users. The MSI installer (`.msi`) is intended for administrators performing enterprise deployments. Do not install both formats.
- **Windows SmartScreen Notice**: Because the binaries are not code-signed, Windows Defender SmartScreen may display a warning ("Windows protected your PC") on first run or installation. To continue, click "**More info**", then click "**Run anyway**".
- **WebView2 Runtime**: If the Microsoft Edge WebView2 runtime is not already installed on your system, the installer will automatically download and install it.

### Linux

- **Formats**: Available as an AppImage or a Debian package (`.deb`).
- **Requirements**: WebKitGTK 4.1 (e.g. `libwebkit2gtk-4.1-0` on Ubuntu 22.04 or later).
- **AppImage**:
  AppImages require FUSE 2, which Ubuntu 22.04 and later do not install by default. Install it first (`libfuse2t64` on Ubuntu 24.04 and later, `libfuse2` on 22.04):
  ```bash
  sudo apt install libfuse2t64
  ```
  Make the downloaded AppImage executable and run it:
  ```bash
  chmod +x PDF*Converter_*_amd64.AppImage
  ./PDF*Converter_*_amd64.AppImage
  ```
- **.deb Package**:
  Install via `apt` to ensure all system dependencies are resolved:
  ```bash
  sudo apt install ./PDF*Converter_*_amd64.deb
  ```

## Building from Source

### Prerequisites

- **Rust**: Toolchain pinned in `rust-toolchain.toml`
- **Node.js**: Version specified in `.nvmrc` (v24, or `>=24.15.0` per `package.json`)
- **npm**: 11 or higher
- **Linux Build Dependencies** (when building on Linux):
  ```bash
  sudo apt-get update
  sudo apt-get install -y \
    libwebkit2gtk-4.1-dev \
    build-essential \
    curl \
    wget \
    file \
    libxdo-dev \
    libssl-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev
  ```
- Note: This repository's `.npmrc` enforces supply-chain security by disallowing packages published less than 7 days ago (`min-release-age=7`) and disabling installation scripts (`ignore-scripts=true`).

### Build Steps

1. Install project dependencies:

   ```bash
   npm ci
   ```

2. Fetch pdfium (downloaded only at build time):

   ```bash
   npm run pdfium:fetch
   ```

3. Start the application in development mode:

   ```bash
   npm run tauri dev
   ```

4. Build production release packages:

   ```bash
   npm run tauri build
   ```

   Bundled packages will be generated under `src-tauri/target/release/bundle/`.

## License

This project is licensed under the [MIT License](LICENSE).

Third-party dependencies and their licenses (including the pdfium rendering engine) can be viewed within the application via the "About this app" button in the top bar.
