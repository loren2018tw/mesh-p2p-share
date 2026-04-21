## ADDED Requirements

### Requirement: 端點狀態更新訊息
WebSocket 通訊協定 MUST 支援端點狀態更新的訊息格式，包含已擁有區塊、上傳連線數及下載連線數。

#### Scenario: 接收狀態更新訊息
- **WHEN** 伺服器收到端點發送的狀態更新訊息
- **THEN** 伺服器解析該訊息並轉交給 P2P 中控中心模組。

### Requirement: P2P 調度指令傳遞
WebSocket 通訊協定 MUST 支援從中控中心向端點發送調度指令（如：連線建議、等待重試）。

#### Scenario: 發送連線建議指令
- **WHEN** 中控中心決定端點 B 應與端點 C 連線
- **THEN** 透過 WebSocket 傳送包含端點 C 資訊的建議訊息給端點 B。
