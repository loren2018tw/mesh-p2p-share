# Centralized Peer Matching Specification

## Purpose

點對點 (P2P) 配對邏輯由中控中心（分享端桌面應用程式）全權主導。中控中心根據各端點回報的狀態（擁有的區塊、當前連線數），主動指派下載任務給各個端點，確保檔案區塊能以最有效率且最分散的方式傳播。

## Requirements

### Requirement: Centralized Assignment Control

中控端 MUST 主動掌握每個下載端的已持有片段清單，以片段為中心遍歷並決定配對。端點不得自行發起區塊請求（除向後兼容外），必須等待中控端發送下載指派指令。

#### Scenario: User Starts Download
- **WHEN** 下載端透過 WebSocket 註冊或發送 `start_download` 訊息
- **THEN** 中控端發送 `file_chunks_info`（包含各區塊 CRC32）給該端點，並觸發配對掃描。

#### Scenario: Host HTTP Round-robin Dispatching
- **WHEN** 中控中心執行配對掃描
- **THEN** 中控中心 MUST 優先檢查是否有可用的 Host HTTP 下載名額。
- **RULE** Host (分享端) 會維護每個檔案的下載游標，按順序將區塊分配給不同的下載端，確保初期片段分佈的稀缺多樣性。
- **RULE** 同一時間內，Host HTTP 指派通常僅允許一個下載端進行下載（全域限制），避免磁碟 I/O 競爭。

#### Scenario: Chunk-Centric Rarest-First Matching
- **WHEN** 執行 WebRTC P2P 配對掃描
- **THEN** 中控端計算每個片段在網路中的持有者數量，從持有者最少的片段開始搜尋。
- **THEN** 尋找持有該片段且上傳容量未滿的來源端點 (非 Host)，以及缺少該片段且下載容量未滿的目標端點，完成指派。

#### Scenario: Least-Progress Downloader Preference
- **WHEN** 多個下載端缺少同一個稀缺片段，且都符合下載條件
- **THEN** 優先將片段分配給「目前持有區塊數最少」的端點，以平衡各端點的進度。

### Requirement: Concurrency Constraints

為確保傳輸穩定性，系統對每個端點的同時連線數設有限制。

#### Scenario: Connection Limits
- **RULE** 每個端點（非 Host）的 WebRTC 同時下載連線數上限為 1。
- **RULE** 每個端點（非 Host）的 WebRTC 同時上傳連線數上限為 1。
- **RULE** Host HTTP 下載不計入上述 WebRTC 連線數限制，但受限於 Host 的 HTTP 分派配額。

### Requirement: Event-Driven Re-matching

配對邏輯 MUST 在狀態變化時立即觸發，以降低傳輸延遲。

#### Scenario: Immediate Re-match
- **WHEN** 收到 `chunk_completed`、`transfer_finished`、`transfer_failed` 或新端點 `Register` 訊息
- **THEN** 立即重新執行 `host_http_dispatch` 與 `find_and_assign_matches` 掃描。

