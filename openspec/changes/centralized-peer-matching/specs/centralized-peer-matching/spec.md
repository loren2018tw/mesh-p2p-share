## ADDED Requirements

### Requirement: Centralized Assignment Control
中控端必須主動計算並分配下載配對，端點不得自行發起區塊請求。

#### Scenario: User Starts Download
- **WHEN** 使用者在下載頁面點擊某個檔案的下載按鈕
- **THEN** 中控端發送 `file_chunks_info` 給該端點，並將該端點加入「待下載佇列」

#### Scenario: Peer-to-Peer Matching
- **WHEN** 中控端執行配對掃描，發現端點 A 缺少分片 X，且端點 B 擁有分片 X 且目前沒有上傳任務
- **THEN** 中控端發送 `assign_download` 指令給端點 A，包含分片資訊與來源端點 B 的 ID

### Requirement: Concurrency Constraints
每個端點同時只能處理一個上傳與一個下載任務。

#### Scenario: Busy Peer Skipping
- **WHEN** 中控端配對時，發現符合條件的來源端點 B 已經有一個進行中的上傳任務
- **THEN** 配對邏輯跳過端點 B，直到其回報任務完成或掃描下一個端點

### Requirement: Event-Driven Re-matching
配對邏輯必須在關鍵事件發生時即時反應。

#### Scenario: Re-match on Completion
- **WHEN** 端點 A 完成一個分片的下載並回報 `chunk_completed`
- **THEN** 中控端立即更新該端點的擁有狀態，並針對缺片端點重新執行配對掃描
