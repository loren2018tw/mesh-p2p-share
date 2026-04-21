## ADDED Requirements

### Requirement: 檔案分塊處理
系統 MUST 能夠將指定的分享檔案以 50MB 為單位進行邏輯分塊。

#### Scenario: 大型檔案分塊
- **WHEN** 處理一個大於 50MB 的檔案
- **THEN** 將檔案依序切分為數個 50MB 的區塊（最後一塊可能小於 50MB），並記錄區塊總數。

### Requirement: 區塊 CRC32 計算
系統 MUST 針對檔案的每一個區塊計算出獨立的 CRC32 檢查碼，供後續驗證使用。

#### Scenario: 產生區塊驗證碼
- **WHEN** 檔案完成分塊處理
- **THEN** 針對每一個區塊計算 CRC32，並儲存各區塊對應的 CRC32 值於記憶體中。
