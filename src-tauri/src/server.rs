use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use tower_http::services::{ServeDir, ServeFile};
use std::net::SocketAddr;

async fn ws_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        if let Ok(msg) = msg {
            if let Message::Text(text) = msg {
                println!("Received WS message: {}", text);
                // 基礎訊息轉發 (目前僅回音，後續實作進階廣播)
                let _ = socket.send(Message::Text(text.into())).await;
            }
        } else {
            break;
        }
    }
}

pub async fn run_server() {
    let serve_dir = ServeDir::new("downloader-dist")
        .not_found_service(ServeFile::new("downloader-dist/index.html"));

    let app = Router::new()
        .nest_service("/", serve_dir)
        .route("/ws", get(ws_handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Web server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
