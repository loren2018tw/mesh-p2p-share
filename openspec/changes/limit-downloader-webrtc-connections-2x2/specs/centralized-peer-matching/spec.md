## MODIFIED Requirements

### Requirement: Centralized Assignment Control

中控端 MUST 主動掌握每個下載端的已持有片段清單與目前方向性連線數（upload/download），以片段為中心遍歷並決定配對。端點不得自行發起區塊請求（除向後兼容外），必須等待中控端發送下載指派指令。

#### Scenario: User Starts Download

- **WHEN** 下載端透過 WebSocket 註冊或發送 `start_download` 訊息
- **THEN** 中控端發送 `file_chunks_info`（包含各區塊 CRC32）給該端點，並觸發配對掃描。

#### Scenario: Host HTTP Round-robin Dispatching

- **WHEN** 中控中心執行配對掃描
- **THEN** 中控中心 MUST 優先檢查是否有可用的 Host HTTP 下載名額。
- **RULE** Host (分享端) 會維護每個檔案的下載游標，按順序將區塊分配給不同的下載端，確保初期片段分佈的稀缺多樣性。
- **RULE** 同一時間內，Host HTTP 指派通常僅允許一個下載端進行下載（全域限制），避免磁碟 I/O 競爭。

#### Scenario: Chunk-Centric Rarest-First Matching with Directional Capacity

- **WHEN** 執行 WebRTC P2P 配對掃描
- **THEN** 中控端計算每個片段在網路中的持有者數量，從持有者最少的片段開始搜尋。
- **THEN** 僅當來源端點上傳連線數小於 2、目標端點下載連線數小於 2，且目標端缺少該片段時，才可建立下載端互傳指派。

#### Scenario: Least-Progress Downloader Preference

- **WHEN** 多個下載端缺少同一個稀缺片段，且都符合下載條件
- **THEN** 優先將片段分配給「目前持有區塊數最少」的端點，以平衡各端點的進度。

### Requirement: Concurrency Constraints

為確保傳輸穩定性，系統對每個端點的同時連線數設有限制。

#### Scenario: Connection Limits

- **RULE** 每個端點（非 Host）的 WebRTC 同時下載連線數上限為 2。
- **RULE** 每個端點（非 Host）的 WebRTC 同時上傳連線數上限為 2。
- **RULE** 中控端在分派下載端互傳前 MUST 先檢查來源與目標端點是否仍符合上述方向性上限。
- **RULE** Host HTTP 下載不計入上述 WebRTC 連線數限制，但受限於 Host 的 HTTP 分派配額。
