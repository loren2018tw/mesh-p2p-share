use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use axum_server::tls_rustls::RustlsConfig;
use rcgen::generate_simple_self_signed;
use std::net::{TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use tower_http::services::{ServeDir, ServeFile};

/// 取得本機對外的 LAN IP（透過 UDP 路由探測，不實際發送封包）
fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip().to_string())
}

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if let Message::Text(text) = msg {
                println!("Received WS message: {}", text);
                let _ = socket.send(Message::Text(text.into())).await;
            }
        } else {
            break;
        }
    }
}

pub async fn run_server(service_url: Arc<Mutex<Option<String>>>) {
    let serve_dir = ServeDir::new("downloader-dist")
        .not_found_service(ServeFile::new("downloader-dist/index.html"));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(serve_dir);

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
        let mut lock = service_url.lock().expect("service_url lock poisoned");
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
