## 1. 初始化專案基礎結構

- [x] 1.1 初始化前端專案 (Vue 3 + Vuetify)
- [x] 1.2 初始化 Tauri Rust 後端設定

## 2. 實作桌面應用程式介面

- [x] 2.1 實作基於 Vuetify 的檔案選擇介面
- [x] 2.2 實作將選擇的檔案資訊透過 Tauri 指令 (Commands) 傳遞至 Rust 後端並儲存狀態

## 3. 實作後端 Web 伺服器 (axum)

- [x] 3.1 引入 axum, tokio, tower-http 等依賴套件
- [x] 3.2 建立 HTTP 路由，伺服下載端的靜態網頁檔案 (SPA)
- [x] 3.3 在 Tauri 啟動時，於背景 (tokio spawn) 啟動 Web 伺服器

## 4. 實作 WebSocket 伺服器與信令處理

- [x] 4.1 在 axum 伺服器中新增 WebSocket 升級路由
- [x] 4.2 實作連線管理，能接收來自下載端網頁的 WebSocket 連線
- [x] 4.3 實作基礎訊息轉發邏輯，作為分享端與下載端間的 WebRTC 信令通道

## 5. 實作下載端入口網頁

- [x] 5.1 建立下載端專用的 Vue + Vuetify 專案或頁面
- [x] 5.2 實作連線至 WebSocket 伺服器的邏輯
- [x] 5.3 顯示基礎連線狀態與介面
