## ADDED Requirements

### Requirement: 本地儲存路徑授權
網頁客戶端 MUST 透過 File System Access API 要求使用者授權並選擇下載檔案的儲存路徑。

#### Scenario: 使用者選擇儲存檔案
- **WHEN** 使用者點擊下載按鈕
- **THEN** 觸發 `showSaveFilePicker()`，待使用者選定路徑與授權後，再開始下載流程。

### Requirement: 被動下載指派
網頁客戶端不再主動挑選區塊，而是 MUST 監聽並執行來自中控中心的下載指派指令 (`SuggestDownload`)。

#### Scenario: 執行下載指令
- **WHEN** 收到 `SuggestDownload` 訊息
- **THEN** 根據訊息中的 `source_peer` 決定是透過 HTTP (從 Host) 或是 WebRTC (從 Peer) 進行區塊下載。

### Requirement: 下載佇列管理
網頁客戶端 MUST 支援多個檔案的下載排程，並依序執行。

#### Scenario: 多檔案下載排隊
- **WHEN** 在已有進行中的下載任務時，使用者點擊另一個檔案的下載
- **THEN** 將新任務加入 `downloadQueue`，待當前任務完成或取消後自動啟動。

