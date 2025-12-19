use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, post};
use axum::{Extension, Json, Router};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};
use url::Url;

use crate::ytdlp::{DownloadOptions, Status, YtdlpClient};

pub async fn routes(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> Router {
    let (tx, _rx) = broadcast::channel::<String>(100);
    let tx = Arc::new(Mutex::new(tx));
    Router::new()
        .route("/", post(download_from_options))
        .route("/cancel", post(cancel_download))
        .route("/check", post(check_url_availability))
        .route("/pause", post(pause_download))
        .route("/ws", any(download_websocket))
        .layer(Extension(tx))
        .with_state(YtdlpClient::new(db, ytdlp_path, download_path).await)
}

async fn download_websocket(
    ws: WebSocketUpgrade,
    Extension(tx): Extension<Arc<Mutex<broadcast::Sender<String>>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx))
}

async fn handle_socket(socket: WebSocket, tx: Arc<Mutex<broadcast::Sender<String>>>) {
    let mut rx = tx.lock().await.subscribe();

    let (mut ws_tx, mut ws_rx) = socket.split();

    tokio::spawn(async move {
        // Broadcast incoming messages from clients to all
        while let Some(Ok(message)) = ws_rx.next().await {
            if let axum::extract::ws::Message::Text(text) = message {
                if let Err(e) = tx.lock().await.send(text.to_string()) {
                    eprintln!("Error broadcasting message: {:?}", e);
                }
            }
        }
    });

    // Broadcast to this client any messages received by the server
    while let Ok(message) = rx.recv().await {
        if let Err(e) = ws_tx
            .send(axum::extract::ws::Message::Text(message.into()))
            .await
        {
            eprintln!("Error sending message to client: {:?}", e);
        }
    }
}

#[derive(Deserialize, Serialize)]
struct Download {
    url: Url,
    options: DownloadOptions,
}

async fn download_from_options(
    State(ytdlp_client): State<YtdlpClient>,
    Json(download): Json<Download>,
) -> StatusCode {
    tokio::task::spawn(async move {
        match ytdlp_client
            .download_from_options(&download.url, &download.options)
            .await
        {
            Ok(status) => match status {
                Status::Canceled | Status::Completed | Status::Paused => {
                    info!("download completed with status: {:?}", status)
                }
                _ => unreachable!("status should not be possible from download_from_options"),
            },
            Err(err) => error!("download failed to err: {:?}, view logs for more details.", err),
        }
    });

    StatusCode::CREATED
}

async fn cancel_download(
    State(ytdlp_client): State<YtdlpClient>,
    Json(url): Json<Url>,
) -> StatusCode {
    match ytdlp_client.cancel_download(url).await {
        Ok(status) => match status {
            Status::Canceled => StatusCode::OK,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        Err(_) => StatusCode::BAD_REQUEST,
    }
}

async fn check_url_availability(
    State(ytdlp_client): State<YtdlpClient>,
    Json(download): Json<Download>,
) -> StatusCode {
    match ytdlp_client
        .check_url_availability(&download.url, &download.options)
        .await
    {
        Ok(status) => {
            info!(
                "download status for url: {}, {}",
                download.url.as_str(),
                status
            );
            match status.success() {
                true => StatusCode::OK,
                false => StatusCode::BAD_REQUEST,
            }
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

async fn pause_download(
    State(ytdlp_client): State<YtdlpClient>,
    Json(url): Json<Url>,
) -> StatusCode {
    match ytdlp_client.pause_download(url).await {
        Ok(status) => match status {
            Status::Paused => StatusCode::OK,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        Err(_) => StatusCode::BAD_REQUEST,
    }
}
