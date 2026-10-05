export const ja = {
  app: {
    title: "PDF Converter",
    aboutButtonAria: "このアプリについて",
    languageLabel: "言語",
    tabs: {
      imagesToPdf: "画像 → PDF",
      pdfToImages: "PDF → 画像",
    },
    navAria: "変換の種類",
  },
  dropZone: {
    titleImages: "画像をここにドロップ",
    titlePdfs: "PDF をここにドロップ",
    descriptionImages:
      "PNG、JPEG、WebP、BMP に対応。フォルダをドロップすると、中の画像をまとめて追加します",
    descriptionPdfs: "フォルダをドロップすると、中の PDF をまとめて追加します",
    addImages: "画像を追加",
    addPdfs: "PDF を追加",
    addFolder: "フォルダを追加",
  },
  listRow: {
    moveUp: "上へ",
    moveDown: "下へ",
    remove: "一覧から外す",
    thumbnailAlt: "{name} のサムネイル",
  },
  footer: {
    noImagesSelected: "画像が選ばれていません",
    noPdfsSelected: "PDF が選ばれていません",
    savePdf: "PDF を保存",
    startConversion: "変換を開始",
    cancel: "キャンセル",
    cancelling: "キャンセル中…",
    progressLabel: "進捗",
  },
  errors: {
    title: "エラー",
    dismiss: "閉じる",
  },
};

export type Translations = typeof ja;
