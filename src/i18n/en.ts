import type { Translations } from "./ja";

export const en: Translations = {
  app: {
    title: "PDF Converter",
    aboutButtonAria: "About this app",
    languageLabel: "Language",
    tabs: {
      imagesToPdf: "Images → PDF",
      pdfToImages: "PDF → Images",
    },
    navAria: "Conversion kind",
  },
  dropZone: {
    titleImages: "Drop images here",
    titlePdfs: "Drop PDFs here",
    descriptionImages:
      "Supports PNG, JPEG, WebP, BMP. Drop a folder to add all images inside.",
    descriptionPdfs: "Drop a folder to add all PDFs inside.",
    addImages: "Add images",
    addPdfs: "Add PDFs",
    addFolder: "Add folder",
  },
  listRow: {
    moveUp: "Move up",
    moveDown: "Move down",
    remove: "Remove from list",
    thumbnailAlt: "Thumbnail of {name}",
  },
  footer: {
    noImagesSelected: "No images selected",
    noPdfsSelected: "No PDFs selected",
    savePdf: "Save PDF",
    startConversion: "Start conversion",
    cancel: "Cancel",
    cancelling: "Cancelling…",
    progressLabel: "Progress",
  },
  job: {
    running: "Converting…",
    runningItem: "Converting: {name}",
    runningItemAndOthers: "Converting: {name} and {count} more",
    done: "Done",
    cancelled: "Cancelled",
    summaryFinished: "Conversion finished: {counts}",
    summaryCancelled: "Cancelled: {counts}",
    summarySeparator: " · ",
    counts: {
      succeeded: "{count} succeeded",
      failed: "{count} failed",
      noPages: "{count} with no pages in range",
      unprocessed: "{count} not processed",
    },
    rowStatus: {
      waiting: "Waiting",
      running: "Converting…",
      ok: "✓ Done",
      failed: "✕ Failed",
      partial: "✕ Partly failed",
      noPages: "No pages in range",
      cancelled: "Cancelled",
      unprocessed: "Not processed",
    },
  },
  errors: {
    title: "Error",
    dismiss: "Dismiss",
  },
};
