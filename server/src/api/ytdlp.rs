use axum::extract::ws::WebSocket;
use axum::extract::{FromRef, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info};
use url::Url;

use crate::core::ytdlp::{self, DownloadOptions, Status, YtdlpClient};

// <----- Types ----->

// <----- AppState ----->

#[derive(Clone)]
struct AppState {
    ytdlp_client: YtdlpClient,
    tx: Arc<Mutex<Sender<String>>>,
}

// <----- DownloadRequest ----->

#[derive(Deserialize, Serialize)]
struct DownloadRequest {
    url: Url,
    options: DownloadOptions,
}

// <----- Impl ----->

// <----- AppState ----->

impl FromRef<AppState> for YtdlpClient {
    fn from_ref(app_state: &AppState) -> YtdlpClient {
        app_state.ytdlp_client.clone()
    }
}

// <----- Routes ----->

pub async fn routes(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> Router {
    let (tx, _) = broadcast::channel::<String>(100);
    let ytdlp_client = YtdlpClient::new(db, ytdlp_path, download_path).await;

    let safe_tx = Arc::new(Mutex::new(tx));
    
    Router::new()
        .route("/", post(download_from_options))
        .route("/cancel", post(cancel_download))
        .route("/check", post(check_url_availability))
        .route("/pause", post(pause_download))
        .route("/urls", get(get_urls))
        .with_state(AppState {
            tx: safe_tx.clone(),
            ytdlp_client,
        })
        .route("/ws", any(download_update_websocket))
        .with_state(safe_tx)
}

// <----- Functions ----->

async fn cancel_download(
    State(ytdlp_client): State<YtdlpClient>,
    Json(url): Json<Url>,
) -> StatusCode {
    info!("canceling download for url: {}", url);
    match ytdlp_client.cancel_download(url.clone()).await {
        Ok(status) => match status {
            Status::Canceled => StatusCode::OK,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
        Err(err) => {
            error!("failed to cancel download for url: {}, err: {:?}", url, err);
            StatusCode::BAD_REQUEST
        }
    }
}

async fn check_url_availability(
    State(ytdlp_client): State<YtdlpClient>,
    Json(download): Json<DownloadRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    match ytdlp_client
        .check_url_availability(&download.url, &download.options)
        .await
    {
        Ok(_) => Ok(StatusCode::OK),
        Err(err) => match err {
            ytdlp::Error::General { err } => {
                Err((StatusCode::INTERNAL_SERVER_ERROR, err.kind().to_string()))
            }
            _ => {
                error!("check failed: {:?}", err);
                Err((StatusCode::BAD_REQUEST, String::from("Bad download")))
            }
        },
    }
}

async fn download_from_options(
    State(app_state): State<AppState>,
    Json(download): Json<DownloadRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (download_kill_tx, download_kill_rx) = mpsc::channel(100);
    if let Err(err) = app_state
        .ytdlp_client
        .add_download(&download.url, &download.options, Some(download_kill_tx))
        .await
    {
        return match err {
            ytdlp::Error::DownloadAlreadyPresent { status } => Err((
                StatusCode::BAD_REQUEST,
                format!("Download already present with status: {}", status),
            )),
            ytdlp::Error::General { err } => {
                Err((StatusCode::INTERNAL_SERVER_ERROR, err.kind().to_string()))
            }
            err => todo!("unhandled case: {:?}", err),
        };
    }

    if let Err(err) = app_state
        .ytdlp_client
        .check_url_availability(&download.url, &download.options)
        .await
    {
        return match err {
            ytdlp::Error::FailedCheck => {
                error!("check failed: {:?}", err);
                // TODO - Remove download on failed check.
                // TODO - Fix message
                Err((
                    StatusCode::BAD_REQUEST,
                    String::from(
                        "Failed to Check URL - Most likely no format(s)/video(s) were found.",
                    ),
                ))
            }
            ytdlp::Error::General { err } => {
                Err((StatusCode::INTERNAL_SERVER_ERROR, err.kind().to_string()))
            }
            err => todo!("unhandled case: {:?}", err),
        };
    }

    let (download_update_tx, mut download_update_rx) = mpsc::channel(100);
    tokio::task::spawn({
        let app_state = app_state.clone();
        let url = download.url.clone();
        async move {
            while let Some(progress_update) = download_update_rx.recv().await {
                let _ = app_state.tx.lock().await.send(
                    json!({
                        "type": "download_update",
                        "data": progress_update
                    })
                    .to_string(),
                );
                // Err doesn't matter - failure means client disconnected - continue sending for if client reconnects.
                // if let Err(err) = app_state.tx.lock().await.send(string) {
                //     error!("failed to send download message to frontend: {}", err);
                //     break;
                // }
            }
            debug!("download update task closed for url: {}", url);
        }
    });

    tokio::task::spawn({
        async move {
            match app_state
                .ytdlp_client
                .download_from_options(
                    &download.url,
                    &download.options,
                    download_kill_rx,
                    Some(download_update_tx),
                )
                .await
            {
                Ok(status) => {
                    let _ = app_state.tx.lock().await.send(
                        json!({
                            "type": "download_status",
                            "data": status
                        })
                        .to_string(),
                    );
                }
                Err(err) => match err {
                    ytdlp::Error::FailedToComplete => todo!(),
                    unhandled_err => todo!("handle error: {:?}", unhandled_err),
                },
            }
        }
    });

    Ok(StatusCode::CREATED)
}

async fn download_update_websocket(
    ws: WebSocketUpgrade,
    State(tx): State<Arc<Mutex<broadcast::Sender<String>>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_download_websocket(socket, tx))
}

async fn get_urls(State(ytdlp_client): State<YtdlpClient>) -> Result<String, StatusCode> {
    match ytdlp_client.get_urls().await {
        Ok(urls) => match serde_json::to_string(&urls) {
            Ok(url_str) => Ok(url_str),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
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
