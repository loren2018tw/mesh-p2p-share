use crate::p2p::{self, SharedState};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State as AxumState,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use axum_server::tls_rustls::RustlsConfig;
use rcgen::generate_simple_self_signed;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex as TokioMutex};
use tower_http::services::{ServeDir, ServeFile};

/// 取得本機對外的 LAN IP（透過 UDP 路由探測，不實際發送封包）
fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

// ── WebSocket P2P 訊息格式定義 ──

/// 客戶端 → 伺服器的訊息
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    /// 註冊為下載端
    #[serde(rename = "register")]
    Register { endpoint_id: String },

    /// 回報端點狀態
    #[serde(rename = "endpoint_status")]
    EndpointStatus {
        endpoint_id: String,
        file_id: String,
        owned_chunks: Vec<u32>,
        upload_count: u32,
        download_count: u32,
    },

    /// 請求下載某區塊
    #[serde(rename = "request_chunk")]
    RequestChunk {
        endpoint_id: String,
        file_id: String,
        chunk_index: u32,
    },

    /// 端點報告開始下載（新模式：中控指派任務）
    #[serde(rename = "start_download")]
    StartDownload {
        endpoint_id: String,
        file_id: String,
    },

    /// 區塊下載完成通知
    #[serde(rename = "chunk_completed")]
    ChunkCompleted {
        endpoint_id: String,
        file_id: String,
        chunk_index: u32,
    },

    /// WebRTC 信令轉發
    #[serde(rename = "webrtc_signal")]
    WebRtcSignal {
        from: String,
        to: String,
        signal: serde_json::Value,
    },

    /// 區塊驗證失敗通知
    #[serde(rename = "chunk_verify_failed")]
    ChunkVerifyFailed {
        #[allow(dead_code)]
        endpoint_id: String,
        file_id: String,
        chunk_index: u32,
        source_peer: String,
    },

    /// 開始傳輸通知（上傳或下載）
    #[serde(rename = "transfer_started")]
    TransferStarted {
        endpoint_id: String,
        #[allow(dead_code)]
        file_id: String,
        #[allow(dead_code)]
        chunk_index: u32,
        is_upload: bool,
    },

    /// 傳輸完成通知（上傳或下載）
    #[serde(rename = "transfer_finished")]
    TransferFinished {
        endpoint_id: String,
        #[allow(dead_code)]
        file_id: String,
        #[allow(dead_code)]
        chunk_index: u32,
        is_upload: bool,
    },

    /// 傳輸失敗通知（目前主要用於 WebRTC timeout）
    #[serde(rename = "transfer_failed")]
    TransferFailed {
        endpoint_id: String,
        file_id: String,
        chunk_index: u32,
        source_peer: String,
        reason: String,
    },
}

/// 伺服器 → 客戶端的訊息
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// 註冊成功
    #[serde(rename = "registered")]
    Registered {
        endpoint_id: String,
        host_endpoint_id: String,
    },

    /// 檔案清單
    #[serde(rename = "file_list")]
    FileList { files: Vec<FileListEntry> },

    /// 下載建議：去找某端點要
    #[serde(rename = "suggest_peer")]
    SuggestPeer {
        file_id: String,
        chunk_index: u32,
        peer_id: String,
    },

    /// 中控指令：下載某分片（包含來源、檔案、分片索引）
    #[serde(rename = "suggest_download")]
    SuggestDownload {
        file_id: String,
        chunk_index: u32,
        source_peer: String,
    },

    /// 請等待重試
    #[serde(rename = "wait_and_retry")]
    WaitAndRetry {
        file_id: String,
        chunk_index: u32,
        wait_seconds: u32,
    },

    /// 檔案區塊資訊（下載時提供）
    #[serde(rename = "file_chunks_info")]
    FileChunksInfo {
        file_id: String,
        file_name: String,
        total_size: u64,
        chunk_count: u32,
        chunks: Vec<ChunkInfoEntry>,
    },

    /// WebRTC 信令轉發
    #[serde(rename = "webrtc_signal")]
    WebRtcSignal {
        from: String,
        signal: serde_json::Value,
    },

    /// 區塊驗證失敗通知（轉發給來源端）
    #[serde(rename = "chunk_verify_failed_notify")]
    ChunkVerifyFailedNotify {
        file_id: String,
        chunk_index: u32,
        reporter: String,
    },
}

#[derive(Debug, Serialize, Clone)]
pub struct FileListEntry {
    pub file_id: String,
    pub file_name: String,
    pub total_size: u64,
    pub chunk_count: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct ChunkInfoEntry {
    pub index: u32,
    pub size: u64,
    pub crc32: u32,
}

#[derive(Debug, Deserialize)]
struct ChunkRequestQuery {
    endpoint_id: Option<String>,
}

/// 伺服器共享狀態
#[derive(Clone)]
pub struct ServerState {
    pub p2p_state: SharedState,
    /// 各端點的 WebSocket 發送通道 (key = endpoint_id)
    pub ws_senders: crate::WsSenders,
    /// 目前執行檔的版本資訊
    pub app_version: String,
    /// 暫時不可用來源端點的冷卻截止時間
    pub source_cooldown_until: Arc<TokioMutex<HashMap<String, Instant>>>,
}

/// HTTP API：取得檔案清單
async fn get_file_list(AxumState(state): AxumState<ServerState>) -> impl IntoResponse {
    let s = state.p2p_state.read().await;
    let files: Vec<FileListEntry> = s
        .shared_files
        .iter()
        .map(|f| FileListEntry {
            file_id: f.file_id.clone(),
            file_name: f.file_name.clone(),
            total_size: f.total_size,
            chunk_count: f.chunk_count,
        })
        .collect();
    Json(files)
}

/// HTTP API：取得程式版本
async fn get_version(AxumState(state): AxumState<ServerState>) -> impl IntoResponse {
    state.app_version
}

/// HTTP API：Host 作為種子時，提供區塊資料下載
async fn get_chunk_data(
    Path((file_id, chunk_index)): Path<(String, u32)>,
    Query(query): Query<ChunkRequestQuery>,
    AxumState(state): AxumState<ServerState>,
) -> impl IntoResponse {
    let endpoint_id = query
        .endpoint_id
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    let endpoint_id = match endpoint_id {
        Some(v) => v,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "missing endpoint_id in query parameter",
            )
                .into_response();
        }
    };

    // 僅允許下載端取得「中控指派且尚未開始」的 HTTP 區塊，避免同端點多重並發
    {
        let mut s = state.p2p_state.write().await;
        let Some(assignment) = s.http_assignments.get_mut(&endpoint_id) else {
            return (
                StatusCode::CONFLICT,
                "no active host HTTP assignment for this endpoint",
            )
                .into_response();
        };

        if assignment.file_id != file_id || assignment.chunk_index != chunk_index {
            return (
                StatusCode::CONFLICT,
                "requested chunk does not match current host HTTP assignment",
            )
                .into_response();
        }

        if assignment.started {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                "host HTTP download already in progress for endpoint",
            )
                .into_response();
        }

        assignment.started = true;

        let host_id = s.host_endpoint_id.clone();
        if let Some(ep) = s.endpoints.get_mut(&host_id) {
            ep.upload_count += 1;
        }
    }

    // 增加 host 的上傳計數
    let result = p2p::read_chunk_data(&state.p2p_state, &file_id, chunk_index).await;

    // 減少 host 的上傳計數
    {
        let mut s = state.p2p_state.write().await;
        let host_id = s.host_endpoint_id.clone();
        if let Some(ep) = s.endpoints.get_mut(&host_id) {
            ep.upload_count = ep.upload_count.saturating_sub(1);
        }
    }

    match result {
        Ok(data) => (
            StatusCode::OK,
            [("content-type", "application/octet-stream")],
            data,
        )
            .into_response(),
        Err(e) => (StatusCode::NOT_FOUND, e).into_response(),
    }
}

/// WebSocket 升級處理
async fn ws_handler(ws: WebSocketUpgrade, AxumState(state): AxumState<ServerState>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// 處理單一 WebSocket 連線
async fn handle_socket(socket: WebSocket, state: ServerState) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(TokioMutex::new(ws_sender));

    // 建立訊息通道
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();
    let endpoint_id: Arc<TokioMutex<Option<String>>> = Arc::new(TokioMutex::new(None));

    // 轉發 ServerMessage 到 WebSocket
    let sender_clone = ws_sender.clone();
    let send_task = tokio::spawn(async move {
        use futures_util::SinkExt;
        while let Some(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                let mut sender = sender_clone.lock().await;
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // 接收客戶端訊息
    use futures_util::StreamExt;
    while let Some(msg) = ws_receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(_) => break,
        };

        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[ws] 無法解析訊息: {e}");
                continue;
            }
        };

        match client_msg {
            ClientMessage::Register { endpoint_id: eid } => {
                // 記錄端點
                {
                    let mut id = endpoint_id.lock().await;
                    *id = Some(eid.clone());
                }
                state
                    .ws_senders
                    .write()
                    .await
                    .insert(eid.clone(), tx.clone());

                // 在 P2P state 中註冊端點
                {
                    let mut s = state.p2p_state.write().await;
                    s.endpoints
                        .entry(eid.clone())
                        .or_insert_with(|| p2p::EndpointState {
                            endpoint_id: eid.clone(),
                            file_id: None,
                            owned_chunks: HashMap::new(),
                            upload_count: 0,
                            download_count: 0,
                        });
                }

                // 回傳註冊成功（含 host endpoint ID，讓下載端知道要向誰做 HTTP 下載）
                let host_endpoint_id = {
                    let s = state.p2p_state.read().await;
                    s.host_endpoint_id.clone()
                };
                let _ = tx.send(ServerMessage::Registered {
                    endpoint_id: eid.clone(),
                    host_endpoint_id,
                });

                // 發送檔案清單
                let files = {
                    let s = state.p2p_state.read().await;
                    s.shared_files
                        .iter()
                        .map(|f| FileListEntry {
                            file_id: f.file_id.clone(),
                            file_name: f.file_name.clone(),
                            total_size: f.total_size,
                            chunk_count: f.chunk_count,
                        })
                        .collect()
                };
                let _ = tx.send(ServerMessage::FileList { files });

                // 觸發配對掃描（新端點加入：HTTP 輪轉 + WebRTC 配對）
                host_http_dispatch(&state).await;
                find_and_assign_matches(&state).await;
            }

            ClientMessage::EndpointStatus {
                endpoint_id: eid,
                file_id,
                owned_chunks,
                upload_count,
                download_count,
            } => {
                let mut should_rematch = false;
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    if !file_id.is_empty() {
                        ep.file_id = Some(file_id.clone());
                        let chunks: HashSet<u32> = owned_chunks.into_iter().collect();
                        let changed = ep
                            .owned_chunks
                            .get(&file_id)
                            .map_or(true, |old| old != &chunks);
                        if changed {
                            should_rematch = true;
                        }
                        ep.owned_chunks.insert(file_id, chunks);
                    }
                    if ep.upload_count != upload_count || ep.download_count != download_count {
                        should_rematch = true;
                    }
                    ep.upload_count = upload_count;
                    ep.download_count = download_count;
                }
                drop(s);

                // 狀態有變化時立即重排，縮短「拿到新片段→能上傳」的反應延遲
                if should_rematch {
                    host_http_dispatch(&state).await;
                    find_and_assign_matches(&state).await;
                }
            }

            ClientMessage::RequestChunk {
                endpoint_id: eid,
                file_id,
                chunk_index,
            } => {
                handle_request_chunk(&state, &tx, &eid, &file_id, chunk_index).await;
            }

            ClientMessage::StartDownload {
                endpoint_id: eid,
                file_id,
            } => {
                // 端點報告開始下載意圖
                // 1. 標記端點正在下載此檔案
                {
                    let mut s = state.p2p_state.write().await;
                    if let Some(ep) = s.endpoints.get_mut(&eid) {
                        ep.file_id = Some(file_id.clone());
                        // 初始化檔案的擁有分片集合（如果還沒有）
                        if !ep.owned_chunks.contains_key(&file_id) {
                            ep.owned_chunks.insert(file_id.clone(), HashSet::new());
                        }
                    }
                }

                // 2. 發送檔案區塊資訊
                send_file_chunks_info(&state, &eid, &file_id).await;

                // 3. 觸發配對掃描（HTTP 輪轉 + WebRTC 配對）
                host_http_dispatch(&state).await;
                find_and_assign_matches(&state).await;
            }

            ClientMessage::ChunkCompleted {
                endpoint_id: eid,
                file_id,
                chunk_index,
            } => {
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    ep.owned_chunks
                        .entry(file_id.clone())
                        .or_insert_with(HashSet::new)
                        .insert(chunk_index);
                }

                // Host HTTP 指派僅在下載端回報完成該片段後才釋放，避免寫入尚未完成就重複分派。
                if let Some(assignment) = s.http_assignments.get(&eid) {
                    if assignment.file_id == file_id && assignment.chunk_index == chunk_index {
                        s.http_assignments.remove(&eid);
                    }
                }
                drop(s);

                // 片段完成：HTTP 輪轉分配下一片段 + WebRTC 配對
                host_http_dispatch(&state).await;
                find_and_assign_matches(&state).await;
            }

            ClientMessage::WebRtcSignal { from, to, signal } => {
                // 轉發 WebRTC 信令到目標端點
                let senders = state.ws_senders.read().await;
                if let Some(target_tx) = senders.get(&to) {
                    let _ = target_tx.send(ServerMessage::WebRtcSignal { from, signal });
                }
            }

            ClientMessage::ChunkVerifyFailed {
                endpoint_id: _,
                file_id,
                chunk_index,
                source_peer,
            } => {
                // 通知來源端驗證失敗
                let senders = state.ws_senders.read().await;
                if let Some(source_tx) = senders.get(&source_peer) {
                    let reporter = endpoint_id.lock().await.clone().unwrap_or_default();
                    let _ = source_tx.send(ServerMessage::ChunkVerifyFailedNotify {
                        file_id,
                        chunk_index,
                        reporter,
                    });
                }
            }

            ClientMessage::TransferStarted {
                endpoint_id: eid,
                is_upload,
                ..
            } => {
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    if is_upload {
                        ep.upload_count += 1;
                    } else {
                        ep.download_count += 1;
                    }
                }
            }

            ClientMessage::TransferFinished {
                endpoint_id: eid,
                is_upload,
                ..
            } => {
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    if is_upload {
                        ep.upload_count = ep.upload_count.saturating_sub(1);
                    } else {
                        ep.download_count = ep.download_count.saturating_sub(1);
                    }
                }
                drop(s);

                // 下載任務結束後通常會釋放容量，立即重排
                if !is_upload {
                    host_http_dispatch(&state).await;
                    find_and_assign_matches(&state).await;
                }
            }

            ClientMessage::TransferFailed {
                endpoint_id,
                file_id,
                chunk_index,
                source_peer,
                reason,
            } => {
                let host_id = {
                    let s = state.p2p_state.read().await;
                    s.host_endpoint_id.clone()
                };

                if !source_peer.is_empty() && source_peer != host_id {
                    let mut cooldown = state.source_cooldown_until.lock().await;
                    cooldown.insert(
                        source_peer.clone(),
                        Instant::now() + Duration::from_secs(20),
                    );
                } else if source_peer == host_id {
                    // Host HTTP 來源失敗時，釋放該端點的 HTTP 指派，讓中控可重新分派。
                    let mut s = state.p2p_state.write().await;
                    if let Some(assignment) = s.http_assignments.get(&endpoint_id) {
                        if assignment.file_id == file_id && assignment.chunk_index == chunk_index {
                            s.http_assignments.remove(&endpoint_id);
                        }
                    }
                }

                println!(
                    "[配對] 來源端點冷卻: src={} requester={} file={} chunk={} reason={}",
                    &source_peer[..8.min(source_peer.len())],
                    &endpoint_id[..8.min(endpoint_id.len())],
                    &file_id[..8.min(file_id.len())],
                    chunk_index,
                    reason
                );

                host_http_dispatch(&state).await;
                find_and_assign_matches(&state).await;
            }
        }
    }

    // 清理斷線的端點
    if let Some(eid) = endpoint_id.lock().await.as_ref() {
        state.ws_senders.write().await.remove(eid);
        let mut s = state.p2p_state.write().await;
        s.endpoints.remove(eid);
        s.http_assignments.remove(eid);
        println!("[ws] 端點 {} 已斷線", eid);
    }

    send_task.abort();
}

/// 中控中心動態排程：處理區塊請求
async fn handle_request_chunk(
    state: &ServerState,
    requester_tx: &mpsc::UnboundedSender<ServerMessage>,
    requester_id: &str,
    file_id: &str,
    chunk_index: u32,
) {
    let s = state.p2p_state.read().await;

    // 先確認檔案存在
    let _file_info = match s.shared_files.iter().find(|f| f.file_id == file_id) {
        Some(f) => f,
        None => return,
    };

    // 如果是第一次請求，先發送檔案區塊資訊
    // （前端也會在點擊下載時主動請求，這裡做為 fallback）

    // 找出擁有該區塊的端點（排除請求者本身）
    let mut candidates: Vec<(&str, u32)> = s
        .endpoints
        .iter()
        .filter(|(id, ep)| {
            *id != requester_id
                && ep
                    .owned_chunks
                    .get(file_id)
                    .map_or(false, |chunks| chunks.contains(&chunk_index))
        })
        .map(|(id, ep)| (id.as_str(), ep.upload_count))
        .collect();

    // 依上傳連線數排序（負載平衡）
    candidates.sort_by_key(|(_id, upload_count)| *upload_count);

    // 最大同時上傳連線數限制
    const MAX_UPLOAD_CONNECTIONS: u32 = 2;

    // 優先尋找目前沒有上傳中的端點
    if let Some((peer_id, upload_count)) = candidates.first() {
        if *upload_count < MAX_UPLOAD_CONNECTIONS {
            let _ = requester_tx.send(ServerMessage::SuggestPeer {
                file_id: file_id.to_string(),
                chunk_index,
                peer_id: peer_id.to_string(),
            });
        } else {
            // 所有候選端點都已滿載
            let _ = requester_tx.send(ServerMessage::WaitAndRetry {
                file_id: file_id.to_string(),
                chunk_index,
                wait_seconds: 3,
            });
        }
    } else {
        // 沒有端點擁有此區塊
        let _ = requester_tx.send(ServerMessage::WaitAndRetry {
            file_id: file_id.to_string(),
            chunk_index,
            wait_seconds: 3,
        });
    }
}

/// 向指定端點發送檔案的完整區塊資訊
#[allow(dead_code)]
pub async fn send_file_chunks_info(state: &ServerState, endpoint_id: &str, file_id: &str) {
    let s = state.p2p_state.read().await;
    let file_info = match s.shared_files.iter().find(|f| f.file_id == file_id) {
        Some(f) => f,
        None => return,
    };

    let msg = ServerMessage::FileChunksInfo {
        file_id: file_info.file_id.clone(),
        file_name: file_info.file_name.clone(),
        total_size: file_info.total_size,
        chunk_count: file_info.chunk_count,
        chunks: file_info
            .chunks
            .iter()
            .map(|c| ChunkInfoEntry {
                index: c.index,
                size: c.size,
                crc32: c.crc32,
            })
            .collect(),
    };

    let senders = state.ws_senders.read().await;
    if let Some(tx) = senders.get(endpoint_id) {
        let _ = tx.send(msg);
    }
}

/// Host 端 HTTP 輪轉分配：依序給每個下載端不同的片段（游標式平均分散）
///
/// 規則：
/// - 同一時間 host 只會指派一個下載端進行 HTTP 下載
/// - 每個檔案維護一個全域游標，指向「下一個要由 host 分發」的片段
/// - 依序掃描各下載端；若端點已有游標位置的片段則跳過（游標不動）
/// - 若端點缺少游標位置的片段則指派，並將游標前進一格
/// - 若有其他候選端點，避免同檔案連續指派給同一個端點
/// - 此邏輯可確保各端點擁有的片段最分散
/// - HTTP 下載不計入端點的下載連線數
async fn host_http_dispatch(state: &ServerState) {
    const HTTP_ASSIGNMENT_TTL: Duration = Duration::from_secs(30);
    const HTTP_STARTED_ASSIGNMENT_TTL: Duration = Duration::from_secs(300);

    let mut to_assign: Vec<(String, String, u32)> = Vec::new();
    let host_id;

    {
        let mut s = state.p2p_state.write().await;
        host_id = s.host_endpoint_id.clone();

        let now = Instant::now();
        let endpoint_ids: HashSet<String> = s.endpoints.keys().cloned().collect();
        s.http_assignments.retain(|endpoint_id, assignment| {
            endpoint_ids.contains(endpoint_id)
                && now.duration_since(assignment.assigned_at)
                    <= if assignment.started {
                        HTTP_STARTED_ASSIGNMENT_TTL
                    } else {
                        HTTP_ASSIGNMENT_TTL
                    }
        });

        // Host HTTP 同時間僅允許一個下載端執行，若已有任務則本輪不再派發。
        if !s.http_assignments.is_empty() {
            // if let Some((ep_id, assignment)) = s.http_assignments.iter().next() {
            // println!(
            //     "[HTTP分派統計] 略過新指派: in_flight={} endpoint={} file={} chunk={} started={}",
            //     s.http_assignments.len(),
            //     &ep_id[..8.min(ep_id.len())],
            //     &assignment.file_id[..8.min(assignment.file_id.len())],
            //     assignment.chunk_index,
            //     assignment.started
            // );
            // }
            return;
        }

        // 先收集所有需要的資料（避免借用衝突）
        let file_ids: Vec<String> = s.shared_files.iter().map(|f| f.file_id.clone()).collect();
        let file_id_set: HashSet<String> = file_ids.iter().cloned().collect();
        s.file_last_http_endpoint.retain(|file_id, endpoint_id| {
            file_id_set.contains(file_id) && endpoint_ids.contains(endpoint_id)
        });
        let chunk_counts: HashMap<String, u32> = s
            .shared_files
            .iter()
            .map(|f| (f.file_id.clone(), f.chunk_count))
            .collect();

        let mut assigned_one = false;

        for file_id in &file_ids {
            let chunk_count = match chunk_counts.get(file_id) {
                Some(&c) if c > 0 => c,
                _ => continue,
            };

            // 1. 取得正在下載此檔案且缺片的端點
            let mut active_downloaders: Vec<(String, HashSet<u32>)> = s
                .endpoints
                .iter()
                .filter(|(id, ep)| {
                    *id != &host_id
                        && ep.file_id.as_deref() == Some(file_id.as_str())
                        && ep
                            .owned_chunks
                            .get(file_id)
                            .map_or(true, |chunks| chunks.len() < chunk_count as usize)
                        && !s.http_assignments.contains_key(id.as_str())
                })
                .map(|(id, ep)| {
                    let owned = ep.owned_chunks.get(file_id).cloned().unwrap_or_default();
                    (id.clone(), owned)
                })
                .collect();

            if active_downloaders.is_empty() {
                continue;
            }

            // 按 ID 排序確保穩定順序，以便輪轉
            active_downloaders.sort_by(|a, b| a.0.cmp(&b.0));

            // 2. 決定下一個該獲得分派的端點 (Round-robin 端點)
            let last_ep = s.file_last_http_endpoint.get(file_id);
            let start_idx = if let Some(last_id) = last_ep {
                active_downloaders
                    .iter()
                    .position(|(id, _)| id == last_id)
                    .map(|pos| (pos + 1) % active_downloaders.len())
                    .unwrap_or(0)
            } else {
                0
            };

            // 3. 從目標端點出發，尋找該端點最需要的片段 (優先參考檔案游標)
            let cursor = *s.file_chunk_cursors.entry(file_id.clone()).or_insert(0);

            for i in 0..active_downloaders.len() {
                let target_idx = (start_idx + i) % active_downloaders.len();
                let (ep_id, owned) = &active_downloaders[target_idx];

                let mut selected_chunk: Option<u32> = None;
                // 從游標開始，找第一個該端點還沒有的片段
                for offset in 0..chunk_count {
                    let candidate = (cursor + offset) % chunk_count;
                    if !owned.contains(&candidate) {
                        selected_chunk = Some(candidate);
                        break;
                    }
                }

                if let Some(chunk_idx) = selected_chunk {
                    // 執行指派
                    to_assign.push((ep_id.clone(), file_id.clone(), chunk_idx));
                    s.file_last_http_endpoint
                        .insert(file_id.clone(), ep_id.clone());
                    s.http_assignments.insert(
                        ep_id.clone(),
                        p2p::HttpChunkAssignment {
                            file_id: file_id.clone(),
                            chunk_index: chunk_idx,
                            started: false,
                            assigned_at: now,
                        },
                    );

                    // 更新檔案游標：下一次從這片之後開始找，增加片段多樣性
                    s.file_chunk_cursors
                        .insert(file_id.clone(), (chunk_idx + 1) % chunk_count);

                    assigned_one = true;
                    break;
                }
            }

            if assigned_one {
                break;
            }
        }
    }

    if to_assign.is_empty() {
        return;
    }

    let mut failed_endpoints: Vec<String> = Vec::new();

    let senders = state.ws_senders.read().await;
    for (ep_id, file_id, chunk_idx) in &to_assign {
        if let Some(tx) = senders.get(ep_id) {
            if tx
                .send(ServerMessage::SuggestDownload {
                    file_id: file_id.clone(),
                    chunk_index: *chunk_idx,
                    source_peer: host_id.clone(),
                })
                .is_err()
            {
                failed_endpoints.push(ep_id.clone());
                continue;
            }

            println!(
                "[HTTP輪轉] 檔案={} 片段={} → {}",
                &file_id[..8.min(file_id.len())],
                chunk_idx,
                &ep_id[..8.min(ep_id.len())]
            );
        } else {
            failed_endpoints.push(ep_id.clone());
        }
    }
    drop(senders);

    if !failed_endpoints.is_empty() {
        let mut s = state.p2p_state.write().await;
        for endpoint_id in failed_endpoints {
            s.http_assignments.remove(&endpoint_id);
        }
    }
}

/// WebRTC 端點間配對：讓瀏覽器端點互相用 WebRTC 分享片段
///
/// 簡化規則（來源端點輪巡）：
/// - 依序檢查每個瀏覽器來源端點，僅挑選目前未上傳者
/// - 對每個來源端點，依序嘗試所有「目前未在 WebRTC 下載」的目標端點
/// - 以目標端點目前正在下載的檔案為準，尋找「來源有、目標缺」的第一個片段並指派
/// - 若該來源端點找不到任何可分享片段，跳過到下一來源端點
/// - 不使用 host 作為 WebRTC 來源（host 透過 host_http_dispatch 獨立處理）
/// - 冷卻中的不穩定來源端點排除在外
async fn find_and_assign_matches(state: &ServerState) {
    const MAX_UPLOAD_CONNECTIONS: u32 = 1;
    const MAX_DOWNLOAD_CONNECTIONS: u32 = 1;

    #[derive(Clone)]
    struct Assignment {
        downloader_id: String,
        file_id: String,
        chunk_idx: u32,
        source_id: String,
        downloader_owned: u32,
        source_owned: u32,
    }

    let now = Instant::now();
    let cooldown_snapshot: HashMap<String, Instant> = {
        let mut cooldown = state.source_cooldown_until.lock().await;
        cooldown.retain(|_, until| *until > now);
        cooldown.clone()
    };

    let s = state.p2p_state.read().await;
    let host_id = s.host_endpoint_id.clone();
    let file_chunk_counts: HashMap<String, u32> = s
        .shared_files
        .iter()
        .map(|f| (f.file_id.clone(), f.chunk_count))
        .collect();

    let mut endpoint_ids: Vec<String> = s
        .endpoints
        .keys()
        .filter(|id| id.as_str() != host_id.as_str())
        .cloned()
        .collect();
    endpoint_ids.sort();

    // 每輪每個來源端點最多指派一筆，避免單一來源壟斷。
    let mut to_assign: Vec<Assignment> = Vec::new();
    let mut assigned_downloaders: HashSet<String> = HashSet::new();

    for source_id in &endpoint_ids {
        let Some(source_ep) = s.endpoints.get(source_id) else {
            continue;
        };

        if source_ep.upload_count >= MAX_UPLOAD_CONNECTIONS {
            continue;
        }

        if cooldown_snapshot
            .get(source_id.as_str())
            .map_or(false, |until| *until > now)
        {
            continue;
        }

        let mut assigned_for_this_source = false;

        for downloader_id in &endpoint_ids {
            if downloader_id == source_id || assigned_downloaders.contains(downloader_id) {
                continue;
            }

            let Some(downloader_ep) = s.endpoints.get(downloader_id) else {
                continue;
            };

            if downloader_ep.download_count >= MAX_DOWNLOAD_CONNECTIONS {
                continue;
            }

            let Some(file_id) = downloader_ep.file_id.as_ref() else {
                continue;
            };

            let Some(&chunk_count) = file_chunk_counts.get(file_id) else {
                continue;
            };

            if chunk_count == 0 {
                continue;
            }

            let downloader_owned_chunks = downloader_ep.owned_chunks.get(file_id);
            let downloader_owned_count = downloader_owned_chunks.map_or(0, |c| c.len() as u32);

            // 已完成的端點不該再被當成下載目標，讓它保留為上傳來源。
            if downloader_owned_count >= chunk_count {
                continue;
            }

            let Some(source_owned_chunks) = source_ep.owned_chunks.get(file_id) else {
                continue;
            };

            if source_owned_chunks.is_empty() {
                continue;
            }

            let mut selected_chunk: Option<u32> = None;

            for chunk_idx in 0..chunk_count {
                let downloader_has =
                    downloader_owned_chunks.map_or(false, |chunks| chunks.contains(&chunk_idx));
                if !downloader_has && source_owned_chunks.contains(&chunk_idx) {
                    selected_chunk = Some(chunk_idx);
                    break;
                }
            }

            let Some(chunk_idx) = selected_chunk else {
                // 這個目標端點缺的片段來源端都沒有，改試下一個目標端點。
                continue;
            };

            assigned_downloaders.insert(downloader_id.clone());
            to_assign.push(Assignment {
                downloader_id: downloader_id.clone(),
                file_id: file_id.clone(),
                chunk_idx,
                source_id: source_id.clone(),
                downloader_owned: downloader_owned_count,
                source_owned: source_owned_chunks.len() as u32,
            });
            assigned_for_this_source = true;
            break;
        }

        if assigned_for_this_source {
            continue;
        }
    }

    drop(s);

    // 發送所有指派訊息
    let senders = state.ws_senders.read().await;
    for c in to_assign {
        if let Some(tx) = senders.get(&c.downloader_id) {
            let _ = tx.send(ServerMessage::SuggestDownload {
                file_id: c.file_id.clone(),
                chunk_index: c.chunk_idx,
                source_peer: c.source_id.clone(),
            });
            println!(
                "[WebRTC配對] 檔案={} 片段={} | {} <- {}（src持有={} dst進度={}）",
                &c.file_id[..8.min(c.file_id.len())],
                c.chunk_idx,
                &c.downloader_id[..8.min(c.downloader_id.len())],
                &c.source_id[..8.min(c.source_id.len())],
                c.source_owned,
                c.downloader_owned
            );
        }
    }
}

pub async fn run_server(
    app_handle: tauri::AppHandle,
    service_url: Arc<TokioMutex<Option<String>>>,
    p2p_state: SharedState,
    ws_senders: crate::WsSenders,
) {
    let app_version = app_handle.package_info().version.to_string();

    let server_state = ServerState {
        p2p_state,
        ws_senders,
        app_version,
        source_cooldown_until: Arc::new(TokioMutex::new(HashMap::new())),
    };

    use tauri::Manager;
    let resource_dir = app_handle
        .path()
        .resource_dir()
        .expect("Failed to get resource dir")
        .join("downloader-dist");

    let serve_dir = ServeDir::new(resource_dir.clone())
        .not_found_service(ServeFile::new(resource_dir.join("index.html")));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/files", get(get_file_list))
        .route("/api/version", get(get_version))
        .route("/api/chunks/{file_id}/{chunk_index}", get(get_chunk_data))
        .fallback_service(serve_dir)
        .with_state(server_state.clone());

    // 偵測 LAN IP 只用於組 URL，server 綁定 0.0.0.0 確保可靠
    let ip_str = local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    let sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        ip_str.clone(),
    ];
    let cert = match generate_simple_self_signed(sans) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[server] 憑證產生失敗: {e}");
            return;
        }
    };
    let cert_pem = match cert.serialize_pem() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[server] 憑證序列化失敗: {e}");
            return;
        }
    };
    let key_pem = cert.serialize_private_key_pem();

    let tls_config = match RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes()).await
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[server] TLS 設定失敗: {e}");
            return;
        }
    };

    // 綁定 0.0.0.0，讓所有介面都能連入
    let listener = match TcpListener::bind(("0.0.0.0", 4343)) {
        Ok(l) => l,
        Err(_) => match TcpListener::bind(("0.0.0.0", 0u16)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[server] 無法綁定 port: {e}");
                return;
            }
        },
    };
    if let Err(e) = listener.set_nonblocking(true) {
        eprintln!("[server] set_nonblocking 失敗: {e}");
        return;
    }

    let port = match listener.local_addr() {
        Ok(a) => a.port(),
        Err(e) => {
            eprintln!("[server] 無法取得 port: {e}");
            return;
        }
    };
    let url = format!("https://{}:{}", ip_str, port);
    {
        let mut lock = service_url.lock().await;
        *lock = Some(url.clone());
    }

    println!("[server] 已啟動，入口網址: {}", url);

    // 啟動背景分派掃描 Timer（每 1.5 秒執行一次）
    let state_clone = server_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(1500));
        loop {
            interval.tick().await;
            host_http_dispatch(&state_clone).await;
            find_and_assign_matches(&state_clone).await;
        }
    });

    let server = match axum_server::from_tcp_rustls(listener, tls_config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[server] 建立 TLS server 失敗: {e}");
            return;
        }
    };
    if let Err(e) = server.serve(app.into_make_service()).await {
        eprintln!("[server] 執行錯誤: {e}");
    }
}
