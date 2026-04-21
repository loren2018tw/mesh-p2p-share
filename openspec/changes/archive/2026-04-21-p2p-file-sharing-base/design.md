## Context

我們正在建立一個基於 Rust 的點對點檔案分享應用程式。此應用程式的目標是允許使用者無需依賴第三方伺服器，直接透過 WebRTC 與其他使用者分享檔案。在第一個階段中，我們需要建立應用的基礎架構，包含提供給使用者的桌面端 GUI (用於選取分享檔案)、提供給下載者的 Web 入口頁面 (Web Server)，以及協助兩端建立 WebRTC 連線的信令交換伺服器 (WebSocket Server)。

## Goals / Non-Goals

**Goals:**
- 建立 Tauri 應用程式外殼作為桌面端。
- 整合 Vue 3 與 Vuetify 3 建立桌面端的 GUI。
- 在 Rust 背景實作內建 HTTP 伺服器 (如 axum)，以提供下載端靜態資源及網頁。
- 在 Rust 背景實作 WebSocket 伺服器，提供下載端與分享端之間的信令交換通道。

**Non-Goals:**
- 實作完整的 WebRTC 點對點連線邏輯與檔案傳輸機制 (將在後續階段實作)。
- 實作進階的權限控管與加密機制 (基礎版暫不考量)。
- NAT 穿透 (STUN/TURN) 的複雜設定 (初期預設在相同網段或具備公開 IP 下測試)。

## Decisions

1. **框架選擇**: 採用 **Tauri** 建立桌面應用程式。
   - *Rationale*: Tauri 相較於 Electron 更輕量，且後端直接使用 Rust，符合效能要求與開發堆疊。
2. **前端技術栈**: 使用 **Vue 3 + Vuetify 3**。
   - *Rationale*: 專案規格要求使用 Vuetify 作為元件框架。提供豐富且現代化的 UI 元件，加速開發。
3. **Rust Web/WebSocket Server**: 選擇 **axum**。
   - *Rationale*: axum 基於 tokio，效能優異，且對 WebSocket 支援良好，為目前 Rust 生態系中主流的 Web 框架之一。
4. **目錄結構與通訊**:
   - Web Server 與 WebSocket Server 將隨 Tauri 應用程式啟動時，在背景執行緒 (tokio runtime) 中啟動。
   - Web Server 負責提供一個 Vue 建立的 SPA 給下載者 (Downloader UI)。

## Risks / Trade-offs

- **Risk: 端口衝突** 
  - Mitigation: 預設使用固定但少用的連接埠 (如 8080/8081)，若被佔用則允許透過 Tauri 介面或配置檔動態更換。
- **Risk: WebSocket 與 Web 伺服器整合複雜度**
  - Mitigation: 在 axum 中可以透過不同的 route 來處理 HTTP 請求與 WebSocket 升級請求，將兩者整合在同一個 port 以簡化架構。
