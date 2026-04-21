## Why

實作 P2P 檔案分享系統的分享端 (rust-app)，為使用者提供一個直觀且安全的介面來分享檔案，並透過 HTTPS 與 WebSocket 確保傳輸的安全性與即時性。

## What Changes

- **HTTPS 服務啟動與埠號管理**: 程式啟動時檢查 4343 埠，若被佔用則隨機選取可用埠。啟動僅提供 HTTPS 的 Web Server，使用自簽署憑證。
- **WebSocket 服務**: 隨程式啟動同步開啟 WebSocket 服務。
- **使用者介面增強**:
    - 啟動時視窗自動最大化。
    - 顯示服務連線網址，並提供一鍵複製按鈕。
    - 提供小型 QR-Code 預覽，點擊後顯示大型 QR-Code 以利下載端掃描。
- **檔案分享管理**: 實作動態檔案管理清單，使用者可隨時新增要分享的檔案，並能在清單中查看基本資訊或移除檔案。

## Capabilities

### New Capabilities
- `sharing-server`: 負責偵測埠號、啟動 HTTPS Web Server (自簽署憑證) 與 WebSocket 服務的後端核心邏輯。
- `sharing-ui`: 負責顯示連線資訊、QR-Code、檔案管理清單以及視窗行為 (最大化) 的前端介面。

### Modified Capabilities
<!-- 無 -->

## Impact

- **Rust 後端**: 需要整合 Axum 或類似框架處理 HTTPS/WS，並實作埠號檢查邏輯。
- **Tauri 配置**: 修改視窗啟動設定。
- **前端 (Vue/Vuetify)**: 實作檔案清單、QR-Code 元件與複製功能。
