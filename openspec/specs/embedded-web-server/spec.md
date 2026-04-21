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
