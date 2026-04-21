## 1. 檔案處理與分享端介面

- [x] 1.1 在 Rust 專案引入 `crc32fast` 等必要依賴。
- [x] 1.2 實作檔案讀取模組，將檔案依據 50MB 進行邏輯分塊並計算各區塊 CRC32 值。
- [x] 1.3 建立記憶體資料結構，儲存分享檔案之元資料（總大小、區塊數、各區塊 CRC32）。
- [x] 1.4 在 Tauri/Vue 桌面介面新增「選擇分享檔案」按鈕與相關處理邏輯，並將選取的檔案傳遞給後端進行分塊處理。

## 2. P2P 中控中心與 WebSocket 信令擴充

- [x] 2.1 擴充 WebSocket 訊息定義，新增 `EndpointStatus`、`RequestChunk`、`SuggestPeer`、`WaitAndRetry`、`ChunkCompleted` 等事件。
- [x] 2.2 實作端點狀態追蹤機制，於記憶體中記錄各連線端點之 ID、已擁有的區塊清單、目前上傳/下載連線數。
- [x] 2.3 實作中控中心動態排程邏輯：接收 `RequestChunk` 時，尋找擁有該區塊的端點，並依據上傳連線數進行排序（負載平衡）。
- [x] 2.4 若有合適端點，回傳 `SuggestPeer`；若全數滿載或無合適端點，回傳 `WaitAndRetry` 指令。

## 3. 使用者端網頁介面與本地檔案授權

- [x] 3.1 實作使用者端網頁，顯示目前可供下載的檔案清單。
- [x] 3.2 整合 File System Access API：當使用者點擊下載時，觸發 `showSaveFilePicker()`，取得寫入權限與 `FileSystemWritableFileStream`。

## 4. 下載端 P2P 調度邏輯

- [x] 4.1 實作 WebSocket 連線邏輯，註冊為下載端，並定時發送 `EndpointStatus` 更新狀態。
- [x] 4.2 實作隨機區塊請求機制：從自己缺少的區塊清單中，隨機挑選並發送 `RequestChunk`。
- [x] 4.3 實作處理 `SuggestPeer`（進行 WebRTC 連線）與 `WaitAndRetry`（延遲重試）的回應邏輯。

## 5. WebRTC 資料傳輸與背壓控制

- [x] 5.1 實作下載端與提供端之間的 WebRTC 連線建立流程（經由 WebSocket 交換 SDP 與 ICE 候選者）。
- [x] 5.2 實作提供端的資料發送邏輯：將 50MB 區塊切割為小單位（如 256KB），並根據 DataChannel 的 `bufferedAmount` 實作背壓暫停/恢復機制。
- [x] 5.3 實作下載端的資料接收邏輯，將接收到的小單位組合成完整的 50MB 區塊緩衝區。

## 6. 資料驗證與儲存

- [x] 6.1 在前端實作或引入 CRC32 計算工具，針對下載完成的 50MB 區塊進行驗證。
- [x] 6.2 若驗證成功，透過 `FileSystemWritableFileStream.write({ type: 'write', position: offset, data: buffer })` 將資料寫入硬碟，並傳送 `ChunkCompleted` 通知中控中心。
- [x] 6.3 若驗證失敗，捨棄該區塊，並重新發起對該區塊的請求。
