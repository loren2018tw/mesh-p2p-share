## Why

當前下載端採用「主動拉取」模式，這在多個端點同時下載時容易造成負載不均，且下載端之間缺乏協調，無法最大化 P2P 協同分享的效果。為了提升整體網路效能並確保傳輸效率，需要將下載配對邏輯改由中控端（分享端）主動分配，實現全局最優調度。

## What Changes

- 下載模式轉變：由原本的「下載端主動請求」改為「中控端主動分配」。
- 流量控制：中控端將嚴格限制每個端點同時只能有一個上傳任務與一個下載任務。
- 主動推送資訊：當使用者點擊下載時，中控端主動推送分片元資料（含 CRC）給下載端。
- 定時/事件驅動配對：中控端會根據網路狀態（定時或端點更新回報）動態計算並下發配對指令。

## Capabilities

### New Capabilities
- `centralized-peer-matching`: 核心配對邏輯，負責計算哪些端點該向誰下載哪個片段。
- `assignment-protocol`: 中控端主動下發下載任務給端點的通訊協定。

### Modified Capabilities
- `p2p-file-sharing`: 修改原有的下載流程，移除由下載端發起的 `request_chunk` 邏輯。

## Impact

- `src-tauri/src/server.rs`: 核心調度邏輯與 WebSocket 訊息處理。
- `src-tauri/src/p2p.rs`: 可能需要新增狀態結構來記錄進行中的配對。
- `src-tauri/downloader-dist/p2p-client.js`: 下載端邏輯需改為監聽「中控指令」而非主動請求。
