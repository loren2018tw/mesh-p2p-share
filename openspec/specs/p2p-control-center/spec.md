## ADDED Requirements

### Requirement: 端點狀態追蹤

中控中心 MUST 記錄所有連線端點的狀態，包含該端點擁有的檔案區塊清單、當前上傳連線數與當前下載連線數。

#### Scenario: 接收端點狀態回報

- **WHEN** 收到端點透過 WebSocket 傳送的狀態更新訊息
- **THEN** 中控中心更新該端點在記憶體中的狀態紀錄。

### Requirement: 動態連線建議

中控中心 MUST 主動掌握每個下載端的片段擁有狀態，並以片段稀缺性為優先序，自行決定應分配哪個片段給哪個下載端；下載端不得主動請求特定片段。

#### Scenario: 稀缺片段優先配對

- **WHEN** 中控中心執行配對掃描
- **THEN** 中控中心 MUST 先計算每個片段目前被多少端點持有，並從持有者最少的片段開始嘗試配對。

### Requirement: Host HTTP 輪轉分派

為確保網路初期有足夠的片段來源，Host MUST 透過 HTTP 協定按順序向各下載端分發片段。

#### Scenario: 檔案游標輪轉
- **RULE** 每個檔案維護一個全域游標 (Cursor)，指向「下一個應由 Host 分發」的片段。
- **RULE** 分派時，依序掃描缺片的下載端，指派游標指向的片段並前進游標，確保片段分佈最為分散。

### Requirement: 異常處理與節流

中控中心必須能處理傳輸失敗與端點斷線，以維持網路健康。

#### Scenario: 來源端點冷卻 (Source Cooldown)
- **WHEN** 收到 `transfer_failed` 報告（非 Host 導致）
- **THEN** 將該來源端點加入冷卻名單（持續 20 秒），期間不再指派其作為上傳來源。

#### Scenario: 端點斷線廣播
- **WHEN** 偵測到 WebSocket 斷線
- **THEN** 中控中心 MUST 向所有剩餘端點廣播 `peer_disconnected` 訊息，讓受影響的下載端能立即中止相關 WebRTC 傳輸並等待重新分派。

