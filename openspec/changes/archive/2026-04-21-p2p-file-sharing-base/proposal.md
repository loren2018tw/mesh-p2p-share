## Why

為了解決點對點檔案分享的需求，我們需要建立一個基於 Rust 的桌面應用程式。這個程式不僅提供使用者介面來選擇要分享的檔案，還要內建 Web Server 作為下載端的入口頁面，以及 WebSocket Server 作為 WebRTC 的信令（Signaling）交換機制，從而實現免除第三方中介伺服器的直接檔案傳輸。

## What Changes

- 建立 Rust 桌面應用程式（基於 Tauri 框架）
- 實作應用程式前端介面（使用 Vuetify），提供檔案選取功能
- 實作內建 Web Server，提供下載端存取的入口網頁（前端同樣使用 Vuetify）
- 實作內建 WebSocket Server，處理下載端與分享端之間的 WebRTC 信令交換
- 此階段僅先實作 Web Server、WebSocket 及應用程式基礎介面，其餘功能後續處理。

## Capabilities

### New Capabilities
- `desktop-app-ui`: 桌面應用程式介面，提供使用者選取欲分享的檔案。
- `embedded-web-server`: 內建 Web Server，提供下載端入口網頁與靜態資源。
- `websocket-signaling`: WebSocket Server，處理 WebRTC 的信令交換。

### Modified Capabilities
(無)

## Impact

- 將引入 Tauri 框架作為桌面應用程式基礎
- 將引入 Vuetify 作為應用程式與下載端網頁的前端元件庫
- 將引入 Rust 的 Web 伺服器框架 (如 axum) 與 WebSocket 相關套件
