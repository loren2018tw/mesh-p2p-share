## ADDED Requirements

### Requirement: 區塊資料傳輸背壓控制
WebRTC 傳輸檔案區塊時，MUST 將 50MB 區塊切割為更小的傳輸單位（例如 256KB），並根據 `bufferedAmount` 動態調整發送速率，避免緩衝區溢出。

#### Scenario: 發送資料緩衝區控制
- **WHEN** WebRTC DataChannel 緩衝區用量超過閾值（如 1MB）
- **THEN** 暫停發送，直到緩衝區低於閾值（如 0.5MB）事件觸發後再繼續發送。

### Requirement: 區塊 CRC32 驗證
下載端完成單一區塊下載後，MUST 計算接收資料的 CRC32，並與中控中心提供的正確 CRC32 進行比對。

#### Scenario: 區塊驗證成功
- **WHEN** 下載完成且 CRC32 比對相符
- **THEN** 透過 WebSocket 通知中控中心該區塊已擁有，並將資料持久化。

### Requirement: 傳輸狀態報告
端點 MUST 向中控中心回報傳輸的生命週期事件，以便進行負載平衡與錯誤復原。

#### Scenario: 回報傳輸事件
- **THEN** 在傳輸開始時發送 `transfer_started`，結束時發送 `transfer_finished`，失敗時發送 `transfer_failed`（包含原因）。

### Requirement: 無進度逾時處理
WebRTC 傳輸若長時間無資料往來，MUST 自動中止以釋放資源。

#### Scenario: 觸發逾時
- **WHEN** WebRTC 下載過程中，超過 30 秒未收到任何資料切片
- **THEN** 強制關閉該連線，並向中控中心回報 `transfer_failed` (reason: idle_timeout)。

