use crate::p2p::{self, SharedState};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State as AxumState,
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
}

/// 伺服器 → 客戶端的訊息
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// 註冊成功
    #[serde(rename = "registered")]
    Registered { endpoint_id: String },

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

/// 伺服器共享狀態
#[derive(Clone)]
pub struct ServerState {
    pub p2p_state: SharedState,
    /// 各端點的 WebSocket 發送通道 (key = endpoint_id)
    pub ws_senders: crate::WsSenders,
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
async fn get_version() -> impl IntoResponse {
    env!("CARGO_PKG_VERSION")
}

/// HTTP API：Host 作為種子時，提供區塊資料下載
async fn get_chunk_data(
    Path((file_id, chunk_index)): Path<(String, u32)>,
    AxumState(state): AxumState<ServerState>,
) -> impl IntoResponse {
    match p2p::read_chunk_data(&state.p2p_state, &file_id, chunk_index).await {
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
async fn ws_handler(
    ws: WebSocketUpgrade,
    AxumState(state): AxumState<ServerState>,
) -> Response {
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
                state.ws_senders.write().await.insert(eid.clone(), tx.clone());

                // 在 P2P state 中註冊端點
                {
                    let mut s = state.p2p_state.write().await;
                    s.endpoints.entry(eid.clone()).or_insert_with(|| {
                        p2p::EndpointState {
                            endpoint_id: eid.clone(),
                            file_id: None,
                            owned_chunks: HashMap::new(),
                            upload_count: 0,
                            download_count: 0,
                        }
                    });
                }

                // 回傳註冊成功
                let _ = tx.send(ServerMessage::Registered {
                    endpoint_id: eid,
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
            }

            ClientMessage::EndpointStatus {
                endpoint_id: eid,
                file_id,
                owned_chunks,
                upload_count,
                download_count,
            } => {
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    ep.file_id = Some(file_id.clone());
                    let chunks: HashSet<u32> = owned_chunks.into_iter().collect();
                    ep.owned_chunks.insert(file_id, chunks);
                    ep.upload_count = upload_count;
                    ep.download_count = download_count;
                }
            }

            ClientMessage::RequestChunk {
                endpoint_id: eid,
                file_id,
                chunk_index,
            } => {
                handle_request_chunk(&state, &tx, &eid, &file_id, chunk_index).await;
            }

            ClientMessage::ChunkCompleted {
                endpoint_id: eid,
                file_id,
                chunk_index,
            } => {
                let mut s = state.p2p_state.write().await;
                if let Some(ep) = s.endpoints.get_mut(&eid) {
                    ep.owned_chunks
                        .entry(file_id)
                        .or_insert_with(HashSet::new)
                        .insert(chunk_index);
                }
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
        }
    }

    // 清理斷線的端點
    if let Some(eid) = endpoint_id.lock().await.as_ref() {
        state.ws_senders.write().await.remove(eid);
        let mut s = state.p2p_state.write().await;
        s.endpoints.remove(eid);
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
    const MAX_UPLOAD_CONNECTIONS: u32 = 5;

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
pub async fn send_file_chunks_info(
    state: &ServerState,
    endpoint_id: &str,
    file_id: &str,
) {
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

pub async fn run_server(
    service_url: Arc<TokioMutex<Option<String>>>,
    p2p_state: SharedState,
    ws_senders: crate::WsSenders,
) {
    let server_state = ServerState {
        p2p_state,
        ws_senders,
    };

    let serve_dir = ServeDir::new("downloader-dist")
        .not_found_service(ServeFile::new("downloader-dist/index.html"));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/files", get(get_file_list))
        .route("/api/version", get(get_version))
        .route("/api/chunks/{file_id}/{chunk_index}", get(get_chunk_data))
        .fallback_service(serve_dir)
        .with_state(server_state);

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
