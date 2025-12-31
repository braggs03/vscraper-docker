use axum::extract::ws::WebSocket;
use axum::extract::{FromRef, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, post};
use axum::{Json, Router};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info};
use url::Url;

use crate::ytdlp::{DownloadOptions, Status, YtdlpClient};

#[derive(Clone)]
struct AppState {
    ytdlp_client: YtdlpClient,
    tx: Arc<Mutex<Sender<String>>>,
}

// support converting an `AppState` in an `ApiState`
impl FromRef<AppState> for YtdlpClient {
    fn from_ref(app_state: &AppState) -> YtdlpClient {
        app_state.ytdlp_client.clone()
    }
}

pub async fn routes(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> Router {
    let (tx, _) = broadcast::channel::<String>(100);
    let ytdlp_client = YtdlpClient::new(db, ytdlp_path, download_path).await;

    let safe_tx = Arc::new(Mutex::new(tx));

    Router::new()
        .route("/", post(download_from_options))
        .route("/cancel", post(cancel_download))
        .route("/check", post(check_url_availability))
        .route("/pause", post(pause_download))
        .with_state(AppState {
            tx: safe_tx.clone(),
            ytdlp_client,
        })
        .route("/ws", any(download_websocket))
        .with_state(safe_tx)
}

async fn download_websocket(
    ws: WebSocketUpgrade,
    State(tx): State<Arc<Mutex<broadcast::Sender<String>>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_download_websocket(socket, tx))
}

async fn handle_download_websocket(socket: WebSocket, tx: Arc<Mutex<broadcast::Sender<String>>>) {
    let mut rx = tx.lock().await.subscribe();

    let (mut ws_tx, mut ws_rx) = socket.split();

    // tokio::spawn(async move {
    //     // Broadcast incoming messages from clients to all
    //     while let Some(Ok(message)) = ws_rx.next().await {
    //         if let axum::extract::ws::Message::Text(text) = message {
    //             if let Err(e) = tx.lock().await.send(text.to_string()) {
    //                 eprintln!("Error broadcasting message: {:?}", e);
    //             }
    //         }
    //     }
    // });

    // Broadcast to this client any messages received by the server
    while let Ok(message) = rx.recv().await {
        if let Err(e) = ws_tx
            .send(axum::extract::ws::Message::Text(message.into()))
            .await
        {
            error!("sending message to client, client disconnected: {}", e);
            return;
        }
    }
}

#[derive(Deserialize, Serialize)]
struct DownloadRequest {
    url: Url,
    options: DownloadOptions,
}

async fn download_from_options(
    State(app_state): State<AppState>,
    Json(download): Json<DownloadRequest>,
) -> StatusCode {
    let (download_update_tx, mut download_update_rx) = mpsc::channel(100);

    tokio::task::spawn(async move {
        while let Some(string) = download_update_rx.recv().await {
            if let Err(err) = app_state.tx.lock().await.send(string) {
                error!("failed to send download message to frontend: {}", err);
            }
        }
    });

    let _ = tokio::task::spawn(async move {
        let _ = app_state
            .ytdlp_client
            .download_from_options(&download.url, &download.options, Some(download_update_tx))
            .await;
    }).await;

    StatusCode::CREATED
}

async fn cancel_download(
    State(ytdlp_client): State<YtdlpClient>,
    Json(url): Json<Url>,
) -> StatusCode {
    match ytdlp_client.cancel_download(url.clone()).await {
        Ok(status) => match status {
            Status::Canceled => StatusCode::OK,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        Err(_) => {
            info!("cancel request for url: {}", url);
            StatusCode::BAD_REQUEST
        }
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


async fn check_url_availability(
    State(ytdlp_client): State<YtdlpClient>,
    Json(download): Json<DownloadRequest>,
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