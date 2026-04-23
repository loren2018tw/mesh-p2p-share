## MODIFIED Requirements

### Requirement: Centralized Assignment Control

中控端 SHALL 主動掌握每個下載端的已持有片段清單，以片段為中心遍歷並決定配對，端點不得自行發起區塊請求；配對演算法 SHALL 依片段稀缺性排序後逐一嘗試，找到合格的來源端與下載端才完成一次指派。

#### Scenario: User Starts Download

- **WHEN** 使用者在下載頁面點擊某個檔案的下載按鈕
- **THEN** 中控端發送 `file_chunks_info` 給該端點，並將該端點加入「待下載佇列」

#### Scenario: Chunk-Centric Rarest-First Matching

- **WHEN** 中控端執行配對掃描
- **THEN** 中控端 SHALL 先計算每個片段被持有的端點數，從持有者最少的片段開始，尋找持有該片段且上傳容量未滿的來源端點，以及缺少該片段且下載容量未滿的目標端點，找到即指派，否則移至下一個片段繼續嘗試。

#### Scenario: Least-Progress Downloader Preference

- **WHEN** 多個下載端都缺少同一個稀缺片段，且都可被分配該片段
- **THEN** 中控端 SHALL 優先把該片段分配給目前已完成片段數較少的下載端。

#### Scenario: No Valid Pair Found for Rarest Chunk

- **WHEN** 持有者最少的片段找不到同時滿足上傳與下載容量的端點配對
- **THEN** 中控端 SHALL 跳過此片段，對持有者次少的片段重複嘗試，直到所有片段都已嘗試。
