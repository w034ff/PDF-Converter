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
  outputDir: {
    title: "保存先フォルダ",
    choose: "フォルダを選ぶ",
    notChosen: "未選択",
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
  job: {
    running: "変換中…",
    runningItem: "変換中：{name}",
    runningItemAndOthers: "変換中：{name} ほか {count} 件",
    done: "完了",
    cancelled: "キャンセルしました",
    summaryFinished: "変換が終わりました：{counts}",
    summaryCancelled: "キャンセルしました：{counts}",
    summarySeparator: " · ",
    counts: {
      succeeded: "成功 {count} 件",
      failed: "失敗 {count} 件",
      noPages: "対象のページなし {count} 件",
      unprocessed: "未処理 {count} 件",
    },
    rowStatus: {
      waiting: "待機",
      running: "変換中…",
      ok: "✓ 完了",
      failed: "✕ 失敗",
      partial: "✕ 一部失敗",
      noPages: "対象のページなし",
      cancelled: "キャンセル",
      unprocessed: "未処理",
    },
  },
  errors: {
    title: "エラー",
    dismiss: "閉じる",
  },
};

export type Translations = typeof ja;
