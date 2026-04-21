## ADDED Requirements

### Requirement: 啟動 Web 伺服器

系統 MUST 在背景啟動一個 HTTP Web 伺服器，以便下載端存取入口網頁。

#### Scenario: 成功啟動 Web 伺服器

- **WHEN** 桌面應用程式啟動時
- **THEN** 系統自動在背景的指定埠口（如 8080）啟動 Web 伺服器。

### Requirement: 提供下載端入口網頁

Web 伺服器 MUST 能夠伺服靜態檔案，包含使用 Vue + Vuetify 開發的下載端入口網頁資源。

#### Scenario: 下載端存取網頁

- **WHEN** 下載端使用者透過瀏覽器存取 Web 伺服器網址
- **THEN** Web 伺服器回傳下載端的 SPA 頁面與所需靜態資源。

### Requirement: 埠號自動管理

系統啟動時必須檢查埠號 4343 是否被佔用。若未被佔用，則使用 4343；若已被佔用，則應自動且隨機選擇一個可用的埠號。

#### Scenario: 埠號 4343 可用

- **WHEN** 程式啟動且 4343 埠未被其他程式佔用
- **THEN** Web Server 應在 4343 埠啟動

#### Scenario: 埠號 4343 被佔用

- **WHEN** 程式啟動且 4343 埠已被佔用
- **THEN** 系統應隨機選擇一個可用埠並在該埠啟動 Web Server

### Requirement: HTTPS 安全服務

Web Server 必須使用自簽署憑證（Self-signed Certificate），且僅提供 HTTPS 服務給下載端使用。

#### Scenario: 啟動 HTTPS 服務

- **WHEN** Web Server 成功啟動
- **THEN** 服務應僅接受經由 HTTPS 協定的請求，並使用自簽署憑證進行加密

### Requirement: WebSocket 服務啟動

系統啟動時必須一併啟動 WebSocket 服務。

#### Scenario: 同步啟動 WebSocket

- **WHEN** 程式啟動
- **THEN** WebSocket 服務應與 Web Server 同時啟動並準備接收連線
