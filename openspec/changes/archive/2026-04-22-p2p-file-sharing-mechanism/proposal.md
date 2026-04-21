## Why

為了實作一個高效且去中心化的檔案分享機制，我們需要開發一套輕量級的 P2P (Peer-to-Peer) 檔案傳輸協定。傳統的單點下載方式容易造成頻寬瓶頸，透過 P2P 架構與動態排程，能讓所有參與下載的端點同時成為提供者，大幅提升整體網路的傳輸效率與穩定性。

## What Changes

- 實作分享端的檔案處理機制，包含檔案選取、切分成 50MB 區塊以及計算各區塊的 CRC32 驗證碼。
- 實作使用者端網頁介面，提供可下載檔案清單，並整合 File System Access API 讓使用者選擇儲存路徑。
- 擴充 WebSocket 伺服器，作為所有 WebRTC 端點的信號交換與狀態回報中樞（回報擁有區塊與連線狀態）。
- 分享端同時作為初始的 WebRTC 檔案提供者（種子），以及整個 P2P 網路的中控中心（動態排程器）。
- 實作下載端的 P2P 邏輯，包含隨機請求缺少的區塊、向中控中心詢問連線建議、與其他端點建立 WebRTC 連線傳輸資料（含背壓控制）。
- 實作區塊的 CRC32 驗證與錯誤處理機制，確保下載檔案的完整性。

## Capabilities

### New Capabilities
- `p2p-control-center`: 分享端作為 P2P 中控中心的邏輯，包含收集端點狀態、動態排程與連線建議。
- `p2p-file-processing`: 分享端將檔案切塊（50MB）與計算 CRC32 的處理機制。
- `p2p-webrtc-transfer`: 下載端與提供端之間的 WebRTC 點對點傳輸協定（包含背壓控制與 CRC32 驗證）。
- `p2p-web-client`: 使用者端的網頁應用，包含檔案清單顯示、File System Access API 整合與 WebRTC 下載邏輯。

### Modified Capabilities
- `desktop-app-ui`: 新增選擇分享檔案的介面與觸發檔案處理的邏輯。
- `websocket-signaling`: 擴充信號協定以支援端點狀態回報（擁有的區塊、連線數）與中控中心的連線調度指令。

## Impact

- 影響目前的 Rust 後端架構，需新增 P2P 中樞邏輯、檔案處理模組。
- 前端 (Vue/Vuetify) 需開發完整的 P2P 下載器介面，並處理 File System Access API 的權限與檔案寫入流程。
- WebSocket 訊息格式與處理邏輯將大幅擴充以支援 P2P 調度。
