use axum::extract::{State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use url::Url;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, broadcast, mpsc};
use tracing::{debug, error, info, trace};

const YTDLP_DOWNLOAD_UPDATE_REGEX: &str = r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?\s+?(\d+(?:\.\d+)?[GMK]iB)\s+at\s+(\d+\.\d+(?:[GMK]i)?B\/s)\s+ETA\s+((\d+:\d+)|(?:Unknown))";
const YTDLP_LOCATION: &str = "yt-dlp";

#[derive(Clone, Serialize)]
struct DownloadProgress {
    url: String,
    percent: String,
    size_downloaded: String,
    speed: String,
    eta: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DownloadOptions {
    #[serde(default = "default_container")]
    container: String,
    #[serde(default = "default_name_format")]
    name_format: String,
    url: Url,
    #[serde(default = "default_quality")]
    quality: String,
    // YTDLP Options
}

fn default_container() -> String {
    String::from("mp4")
}

fn default_name_format() -> String {
    String::from("%(title)s.%(ext)s")
}

fn default_quality() -> String {
    String::from("best")
}

pub fn routes(db: SqlitePool, download_path: PathBuf) -> Router {
    let (tx, _rx) = broadcast::channel(100);
    Router::new()
        .route("/ws", get(download))
        .with_state(Arc::new(Mutex::new(YtdlpClient::new(db, download_path))))
}

async fn download(
    ws: WebSocketUpgrade,
    State(state): State<Arc<Mutex<YtdlpClient>>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

struct YtdlpClient {
    db: SqlitePool,
    download_path: PathBuf,
    downloads: HashMap<Url, Status>,
}

enum Status {
    None,
    Canceled, 
    Paused,
    Running { tx: Sender<Signal> },
}

enum Signal {
    Cancel,
    Pause,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    NotDownloading,
    FailedToHalt
}

impl YtdlpClient {
    pub fn new(db: SqlitePool, download_path: PathBuf) -> YtdlpClient {
        YtdlpClient { db: db.clone(), download_path, downloads: init_from_db(db) }
    }

    async fn download_from_options(&self, options: DownloadOptions) -> Result<StatusCode> {
        let (tx, mut rx) = mpsc::channel(100); // Used to communicate kill and pause.

        self.add_download_handler(options.url, tx);

        debug!("checking url availability for: {}", options.url);
        match self
            .check_url_availability(&options)
            .await
        {
            Ok(exit_status) => {
                if exit_status.success() {
                    // TODO: Parse stderr to provide exact error caused by yt-dlp.
                    // Return generic error in place of other errors
                } else {
                }
                // WEBSOCKET: Emission::YtdlpUrlUpdate
            }
            Err(err) => match err.kind() {
                err => error!("executing command: {}", err),
            },
        }

        let download_path = self.download_path;

        debug!("downloading from url");
        let mut child = Command::new(YTDLP_LOCATION)
            .arg("--newline")
            .arg("-f")
            .arg(options.quality)
            .arg("-o")
            .arg(download_path)
            .arg(options.url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        debug!(
            "spawned ytdlp download from url: {}, with pid: {}",
            options.url,
            child
                .id()
                .map_or("unknown".to_string(), |code| code.to_string())
        );

        let stderr = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stderr).lines();

        let regex = Regex::new(YTDLP_DOWNLOAD_UPDATE_REGEX).unwrap();
        while let Ok(Some(line)) = reader.next_line().await {
            trace!("ytdlp: {}", line);
            match rx.try_recv() {
                Ok(status) => {
                    let pid = child
                        .id()
                        .map_or("unknown".to_string(), |code| code.to_string());
                    debug!(
                        "received kill signal for url: {}, pid: {}",
                        options.url, pid
                    );
                    match child.kill().await {
                        Ok(_) => {
                            info!(
                                "successfully killed child for url: {}, pid: {}",
                                options.url, pid
                            );
                            match child.wait().await {
                                Ok(exit_status) => {
                                    debug!(
                                        "killed zombie child for url: {}, pid: {}, exit code: {}",
                                        options.url, pid, exit_status
                                    );
                                }
                                Err(err) => {
                                    error!(
                                        "failed to kill zombie child for url: {}, pid: {}, err: {}",
                                        options.url, pid, err
                                    );
                                }
                            }
                        }
                        Err(err) => error!(
                            "failed to kill child for url: {}, pid: {} err: {}",
                            options.url, pid, err
                        ),
                    }
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
            if regex.is_match(&line) {
                if let Some(captures) = regex.captures(&line) {
                    let url = options.url.clone();
                    let percent = String::from(&captures[1]);
                    let size_downloaded = String::from(&captures[2]);
                    let speed = String::from(&captures[3]);
                    let eta = String::from(&captures[4]);

                    // WEBSOCKET: Emission::YtdlpDownloadUpdate
                }
            }
        }

        match child.wait().await {
            Ok(status) => {
                // WEBSOCKET: Emission::YtdlpDownloadFinish
            }
            Err(err) => error!(
                "download with url: {}, failed with err: {}",
                options.url, err
            ),
        }
    }

    async fn add_download_handler(&mut self, url: Url, tx: Sender<Signal>) {
        self.downloads.insert(url, Status::Running { tx });
    }

    pub async fn cancel_download(&self, url: Url) -> Result<()> {
        match self.downloads.get(&url) {
            Some(status) => {
                match status {
                    Status::Running { tx } => {
                        match tx.send(Signal::Cancel).await {
                            Ok(_) => Ok(()),
                            Err(err) => Err(Error::FailedToHalt),
                        }
                    },
                    _ => Err(Error::NotDownloading)
                }
            },
            None => Err(Error::NotDownloading),
        }
    }

    async fn check_url_availability(
        &self,
        options: &DownloadOptions,
    ) -> std::result::Result<ExitStatus, std::io::Error> {
        Command::new(YTDLP_LOCATION)
            .arg("--simulate")
            .arg(&options.url.as_str())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .status()
            .await
    }

    async fn download_best_quality(
        &self,
        state: State<SqlitePool>,
        options: DownloadOptions,
    ) -> Result<StatusCode> {
        self.download_from_options(
            DownloadOptions {
                quality: String::from("bestvideo"),
                ..options
            },
        )
        .await
    }
}

fn init_from_db(db: SqlitePool) -> HashMap<Url, Status> {
    
}