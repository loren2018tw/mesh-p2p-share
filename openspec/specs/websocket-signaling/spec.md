## ADDED Requirements

### Requirement: 端點狀態更新訊息
WebSocket 通訊協定 MUST 支援端點狀態更新的訊息格式，包含已擁有區塊、上傳連線數及下載連線數。

#### Scenario: 接收狀態更新訊息
- **WHEN** 伺服器收到端點發送的 `endpoint_status` 訊息
- **THEN** 伺服器解析該訊息並觸發中控中心的配對邏輯。

### Requirement: P2P 調度指令傳遞
WebSocket 通訊協定 MUST 支援從中控中心向端點發送調度指令，用於主動引導傳輸。

#### Scenario: 發送下載指派指令
- **WHEN** 中控中心決定端點 A 應從端點 B 下載片段 X
- **THEN** 向端點 A 發送 `SuggestDownload` 訊息，包含 `file_id`、`chunk_index` 與 `source_peer`。

### Requirement: 斷線清理與通知
伺服器 MUST 處理端點離線事件，並即時知會受影響的端點。

#### Scenario: 廣播端點離線
- **WHEN** 端點斷開連線
- **THEN** 伺服器將其從活躍清單移除，並向其他所有端點發送 `peer_disconnected` 訊息。

