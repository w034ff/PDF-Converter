# PDF Converter

[English](README.md)

PDF Converter は、画像（PNG、JPEG、WebP、BMP）から PDF への変換、および PDF から画像（PNG、JPEG）への変換を手元で行うオープンソースのデスクトップアプリケーションです。
外部のオンライン変換サービスにファイルをアップロードすることなく、すべての処理がお使いの PC 上で完結します。対応 OS は Windows および Linux（x64）です。

## 主な機能

- **画像 → PDF**: 元画像の画質を落とさずに PDF へ変換。JPEG は再圧縮せずにそのまま埋め込み、透過 PNG や WebP、BMP も可逆圧縮で透過を維持して変換します。EXIF の向き（Orientation）を反映し、ページの大きさは「画像に合わせる」と「A4」（自動・縦・横の向き選択に対応）から選択できます。
- **結合と並べ替え**: 複数の画像をまとめて 1 つの PDF に結合。ドラッグ＆ドロップやキーボード操作（↑↓、Alt + ↑ / ↓）でページの順序を柔軟に並べ替えられます。行を個別に外すことも可能です。
- **PDF → 画像**: PDF の各ページを PNG または JPEG 画像として書き出し。解像度は 72 dpi、150 dpi、300 dpi から選択可能で、白背景の上に描画します。
- **ページ範囲の指定**: 全ページの変換に加え、「1-3, 5」のようなページ範囲を指定して変換可能。単一の PDF ではサムネイル一覧を見ながらクリックでページを選択でき、複数の PDF に対しても同じ範囲を一括で適用できます。
- **一括変換とキャンセル**: 複数のファイルを指定した保存先フォルダへまとめて変換。マルチコアを活用して並列に処理します。変換はいつでもキャンセルでき、保存先に不完全なファイルが残ることはありません。
- **出力名の重複防止**: 保存先に同名のファイルがすでにある場合や変換内で重複する場合、連番（`name (1).pdf` や `name_p1 (1).png` など）を付与して既存ファイルを上書きしません。
- **多言語対応**: 日本語と英語の UI に対応。言語の設定は次回起動時にも引き継がれます。
- **設定の保持**: 最後に使用した変換設定（ページの大きさ、出力形式、解像度、保存先フォルダなど）を記憶し、次回起動時に自動で復元します。
- **パスワード保護された PDF について**: パスワードで保護された PDF は開けません。
- **外部通信なし**: アプリ自体はネットワーク通信を行いません（テレメトリやアップデート確認もありません）。ただし Windows では、画面の表示に使う Microsoft Edge WebView2 ランタイムが、自身で Microsoft のサービスに接続することがあります。これはアプリから止められません。

## インストール

[Releases](https://github.com/w034ff/PDF-Converter/releases) ページから、お使いの OS に合わせたインストーラーまたは実行ファイルをダウンロードしてください。

### Windows

- **インストーラー形式**: 通常は NSIS インストーラー（`.exe`）をおすすめします。MSI インストーラー（`.msi`）は組織内での一括配布を行う管理者向けです。両方をインストールしないでください。
- **SmartScreen の警告について**: 本アプリはコード署名を行っていないため、初回起動時やインストール時に Windows Defender SmartScreen の警告（「Windows によって PC が保護されました」）が表示される場合があります。その場合は「**詳細情報**」をクリックし、「**実行**」を選択して進めてください。
- **WebView2 ランタイム**: システムに Microsoft Edge WebView2 ランタイムがインストールされていない場合、インストーラーによって自動的に導入されます。

### Linux

- **形式**: AppImage および Debian パッケージ（`.deb`）を用意しています。
- **動作要件**: WebKitGTK 4.1（`libwebkit2gtk-4.1-0` 等）が必要です（Ubuntu 22.04 以降対応）。
- **AppImage**:
  AppImage の実行には FUSE 2 が必要ですが、Ubuntu 22.04 以降には標準で入っていません。先にインストールしてください（Ubuntu 24.04 以降は `libfuse2t64`、22.04 は `libfuse2`）。
  ```bash
  sudo apt install libfuse2t64
  ```
  そのうえで、ダウンロードした AppImage に実行権限を付与して起動してください。
  ```bash
  chmod +x PDF*Converter_*_amd64.AppImage
  ./PDF*Converter_*_amd64.AppImage
  ```
- **.deb パッケージ**:
  `apt` を使って依存関係を含めてインストールします。
  ```bash
  sudo apt install ./PDF*Converter_*_amd64.deb
  ```

## ソースコードからのビルド

### 必要な環境

- **Rust**: `rust-toolchain.toml` で指定されているツールチェーン
- **Node.js**: `.nvmrc` で指定されているバージョン（v24、または `package.json` の要件 `>=24.15.0`）
- **npm**: 11 以上
- **Linux 依存パッケージ**（Linux 環境でビルドする場合）:
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
- なお、本リポジトリの `.npmrc` ではセキュリティ確保のため、公開後 7 日未満のバージョンを導入しない設定（`min-release-age=7`）と、インストール時スクリプトを実行しない設定（`ignore-scripts=true`）が有効になっています。

### 手順

1. 依存関係のインストール:

   ```bash
   npm ci
   ```

2. pdfium の取得（ビルド時にのみ取得します）:

   ```bash
   npm run pdfium:fetch
   ```

3. 開発モードでの起動:

   ```bash
   npm run tauri dev
   ```

4. リリース用パッケージのビルド:

   ```bash
   npm run tauri build
   ```

   ビルド成果物は `target/release/bundle/` 配下に生成されます。

## ライセンス

本ソフトウェアは [MIT License](LICENSE) のもとで公開されています。

使用している第三者ライブラリのライセンス一覧（PDF 描画エンジンの pdfium を含む）は、アプリ内の上部バーにある「このアプリについて」ボタンから確認できます。
