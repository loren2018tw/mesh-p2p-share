## ADDED Requirements

### Requirement: 本地儲存路徑授權
網頁客戶端 MUST 透過 File System Access API 要求使用者授權並選擇下載檔案的儲存路徑。

#### Scenario: 使用者選擇儲存檔案
- **WHEN** 使用者點擊下載按鈕
- **THEN** 觸發 `showSaveFilePicker()`，待使用者選定路徑與授權後，再開始下載流程。

### Requirement: 隨機區塊請求
網頁客戶端 MUST 根據自身缺少的區塊清單，隨機挑選一個區塊向中控中心發出下載請求。

#### Scenario: 發出區塊請求
- **WHEN** 客戶端準備下載下一個區塊
- **THEN** 從尚未擁有的區塊清單中亂數選擇一個索引，並發送請求至中控中心。
