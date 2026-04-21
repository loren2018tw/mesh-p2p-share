## ADDED Requirements

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
