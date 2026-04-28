# Mesh P2P Share

Mesh P2P Share 是一個基於 WebRTC 與 Rust/Tauri 開發的點對點 (P2P) 檔案分享工具。它允許使用者透過瀏覽器，安全且高速地與其他裝置分享大型檔案。

## 功能特色

- **點對點傳輸 (P2P)**：檔案直接在設備間傳輸，無需透過第三方伺服器中轉，保護您的隱私。
- **跨平台支援**：提供 Windows 與 Linux 原生桌面應用程式。
- **內建網頁伺服器**：分享端會自動啟動內建的 HTTPS 網頁伺服器，下載端只需開啟瀏覽器連線即可，無須安裝額外軟體。
- **自動分塊與校驗**：支援大檔案分享，透過自動分塊與 CRC32 校驗確保檔案傳輸完整性。
- **QR Code 快速分享**：內建 QR Code 生成功能，可透過手機掃描快速連線下載。
- **多點下載加速**：採用類似 BT 的多點下載機制，當有多個下載端擁有相同的檔案區塊時，可互相支援加速下載。特別適合同時間分享大檔案給多台電腦。以需要將檔案傳送給18臺主機實測所需時間：
  - 傳統 http server: 1G檔案 2分33秒 5G檔案 13分47秒
  - mesh-p2p-share: 1G檔案 2分38秒 5G檔案 8分30秒

## 程式開發

請先確認您的開發環境已安裝 [Node.js](https://nodejs.org/)、[pnpm](https://pnpm.io/) 以及 [Rust](https://www.rust-lang.org/) 與 Tauri 開發環境所需的相關依賴。

### 安裝依賴套件

```bash
pnpm install
```

### 開發模式

啟動具有熱重載 (Hot-Module Replacement) 功能的開發伺服器：

```bash
pnpm tauri dev
```

## 建構說明

當您準備好將應用程式打包發布時，可以使用以下指令建構生產環境版本。該指令會自動編譯前端與 Rust 後端，並產生對應平台的執行檔與安裝檔。

```bash
pnpm tauri build
```

建構完成的檔案將會放置於 `src-tauri/target/release/bundle/` 目錄下。
