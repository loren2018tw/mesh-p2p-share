## ADDED Requirements

### Requirement: 區塊資料傳輸背壓控制
WebRTC 傳輸檔案區塊時，MUST 將 50MB 區塊切割為更小的傳輸單位（例如 256KB），並根據 `bufferedAmount` 動態調整發送速率，避免緩衝區溢出。

#### Scenario: 發送資料緩衝區控制
- **WHEN** WebRTC DataChannel 緩衝區用量超過閾值
- **THEN** 暫停發送，直到緩衝區清空事件觸發後再繼續發送剩餘資料。

### Requirement: 區塊 CRC32 驗證
下載端完成單一區塊下載後，MUST 計算接收資料的 CRC32，並與中控中心提供的正確 CRC32 進行比對。

#### Scenario: 區塊驗證成功
- **WHEN** 下載完成且 CRC32 比對相符
- **THEN** 透過 WebSocket 通知中控中心該區塊已擁有，並將資料寫入硬碟。

#### Scenario: 區塊驗證失敗
- **WHEN** 下載完成但 CRC32 比對不符
- **THEN** 捨棄該區塊資料，通知傳送方驗證失敗，並重新向中控中心請求該區塊。
