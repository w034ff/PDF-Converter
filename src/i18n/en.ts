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
  errors: {
    title: "Error",
    dismiss: "Dismiss",
  },
};
