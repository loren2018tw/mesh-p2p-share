## ADDED Requirements

### Requirement: 啟動 WebSocket 伺服器
系統 MUST 在背景啟動一個 WebSocket 伺服器，負責處理分享端與下載端間的 WebRTC 信令交換。

#### Scenario: 成功啟動 WebSocket 伺服器
- **WHEN** 桌面應用程式啟動時
- **THEN** 系統自動在背景啟動 WebSocket 伺服器，準備接受信令連線。

### Requirement: 處理客戶端連線
WebSocket 伺服器 MUST 允許下載端透過 WebSocket 建立連線，以便後續進行 WebRTC 握手。

#### Scenario: 客戶端成功連線
- **WHEN** 下載端的網頁發起 WebSocket 連線請求
- **THEN** 伺服器接受連線並將該連線加入信令管理的通道中。
