## ADDED Requirements

### Requirement: 顯示應用程式介面
系統 MUST 提供一個基於 Tauri 與 Vuetify 的桌面端圖形使用者介面。

#### Scenario: 啟動應用程式
- **WHEN** 使用者啟動 Tauri 應用程式
- **THEN** 系統顯示包含主要控制項的桌面視窗。

### Requirement: 檔案選取功能
系統 MUST 允許使用者透過介面選擇系統內的檔案作為分享項目。

#### Scenario: 成功選取檔案
- **WHEN** 使用者點擊「選擇檔案」按鈕並從系統對話框中選擇檔案
- **THEN** 系統將所選檔案加入待分享清單並顯示於介面上。
