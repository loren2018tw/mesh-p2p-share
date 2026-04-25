## ADDED Requirements

### Requirement: 選取分享檔案
桌面應用程式介面 MUST 提供按鈕讓使用者透過系統對話框選取要分享的檔案。

#### Scenario: 使用者新增分享檔案
- **WHEN** 使用者點擊新增檔案按鈕並在對話框中選擇檔案
- **THEN** 將檔案資訊加入分享清單，並觸發後端進行檔案分塊與 CRC32 處理。

### Requirement: QR Code 快速分享
系統 MUST 為下載端入口網址生成對應的 QR Code，方便行動裝置掃描連線。

#### Scenario: 放大查看 QR Code
- **WHEN** 使用者點擊 UI 上的 QR Code 預覽圖
- **THEN** 以彈出視窗 (Dialog) 顯示高解析度放大版的 QR Code。

### Requirement: 即時系統狀態面板
桌面應用程式 MUST 顯示當前 P2P 網路的運行統計。

#### Scenario: 檢視連線統計
- **THEN** UI 應包含「目前連線終端數」、「目前分享中 (Seed)」與「目前下載中 (Leech)」的計數，並每 2 秒更新一次。

### Requirement: 軟體版本資訊
UI MUST 在標題列或適當位置顯示當前程式的版本號。

