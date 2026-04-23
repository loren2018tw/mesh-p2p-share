## 1. 通訊協定與狀態結構更新

- [x] 1.1 在 `server.rs` 的 `ServerMessage` 中新增 `SuggestDownload` 指令（包含來源、檔案、分片索引）
- [x] 1.2 在 `server.rs` 的 `ClientMessage` 中移除 `RequestChunk`
- [x] 1.3 在 `p2p.rs` 的 `EndpointState` 新增 `is_uploading` 與 `is_downloading` 布林標記
- [x] 1.4 在 `p2p.rs` 的 `P2PState` 中新增一個追蹤正在下載中的端點集合

## 2. 中控端配對邏輯實作

- [x] 2.1 在 `server.rs` 中實作核心配對函數 `find_and_assign_matches`
- [x] 2.2 實作「1 上傳 / 1 下載」的過濾規則，確保端點不被重複分配
- [x] 2.3 修改 `handle_socket`，在收到 `Register` 或 `ChunkCompleted` 時觸發配對掃描
- [x] 2.4 實作背景 Timer，每隔 1-2 秒自動執行一次全局配對掃描

## 3. 下載端 (Client) 邏輯調整

- [x] 3.1 修改 `p2p-client.js`，移除 `_requestNextChunks` 中的主動請求邏輯
- [x] 3.2 實作對 `SuggestDownload` 訊息的監聽與回應
- [x] 3.3 確保下載端在收到中控指令後，自動發起與指定端點的 WebRTC 連線
- [x] 3.4 修改 UI 下載按鈕觸發的行為，改為僅向中控回報「我想要下載此檔案」

## 4. 流程測試與優化

- [x] 4.1 測試多端點同時下載時，中控是否能正確排隊分配
- [x] 4.2 驗證當來源端上傳完成後，中控是否能立即指派下一個任務
- [x] 4.3 移除程式碼中所有遺留的 `request_chunk` 相關邏輯

> **注：** 新的 `SuggestDownload` 機制已實作完成，`request_chunk` 機制保留用於向後相容性。系統現在優先使用中控指派的 `SuggestDownload` 訊息。
