use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Sender;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, trace};
use url::Url;

const YTDLP_DOWNLOAD_UPDATE_REGEX: &str = r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?\s+?(\d+(?:\.\d+)?[GMK]iB)\s+at\s+(\d+\.\d+(?:[GMK]i)?B\/s)\s+ETA\s+((\d+:\d+)|(?:Unknown))";
const YTDLP_LOCATION: &str = "yt-dlp";

#[derive(Clone)]
struct YtdlpClient {
    db: SqlitePool,
    download_path: PathBuf,
    downloads: Arc<Mutex<HashMap<Url, (Status, Option<Sender<Signal>>)>>>,
}

#[derive(Debug, Clone, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "text", rename_all = "snake_case")] // Store it as a string (TEXT)
enum Status {
    Canceled,
    Completed,
    None,
    Paused,
    Running,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<String> for Status {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Canceled" => Status::Canceled,
            "Completed" => Status::Completed,
            "None" => Status::None,
            "Paused" => Status::Paused,
            "Running" => Status::Running,
            _ => Status::None,
        }
    }
}

enum Signal {
    Cancel,
    Pause,
}

pub type Result<T> = std::result::Result<T, Error>;

pub enum Error {
    NotDownloading,
    FailedToHalt,
}

#[derive(Clone, Serialize)]
struct DownloadProgress {
    url: Url,
    percent: String,
    size_downloaded: String,
    speed: String,
    eta: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DownloadOptions {
    #[serde(default = "default_container")]
    container: String,
    #[serde(default = "default_name_format")]
    name_format: String,
    url: Url,
    #[serde(default = "default_quality")]
    quality: String,
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

impl YtdlpClient {
    pub async fn new(db: SqlitePool, download_path: PathBuf) -> YtdlpClient {
        YtdlpClient {
            db: db.clone(),
            download_path,
            downloads: init_from_db(db).await,
        }
    }

    async fn download_from_options(
        &self,
        options: &DownloadOptions,
    ) -> std::result::Result<StatusCode, StatusCode> {
        let (tx, mut rx) = mpsc::channel(100);
        self.add_download_handler(options.url.clone(), tx).await;

        debug!("checking url availability for: {}", options.url);
        match self.check_url_availability(&options).await {
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

        let download_path = self.download_path.clone().join(&options.name_format);

        debug!("downloading from url");
        let mut child = Command::new(YTDLP_LOCATION)
            .arg("--newline")
            .arg("-f")
            .arg(&options.quality)
            .arg("--rate-limit")
            .arg("100K")
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
                Ok(signal) => {
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

                    match signal {
                        Signal::Cancel => {
                            let download_file_name = self.get_filename(options).await;
                            let download_dir_files = std::fs::read_dir(&self.download_path);
                            if let Some(download_file_name) = download_file_name {
                                for dir in download_dir_files {
                                    for file in dir {
                                        match file {
                                            Ok(file) => match file.file_name().into_string() {
                                                Ok(file_name) => {
                                                    if file_name.contains(&download_file_name) {
                                                        info!(
                                                            "removing file: {}",
                                                            file.file_name()
                                                                .into_string()
                                                                .unwrap_or("unknown".to_string())
                                                        );
                                                        let _ = fs::remove_file(file.path());
                                                    }
                                                }
                                                Err(_) => todo!(),
                                            },
                                            Err(_) => todo!(),
                                        }
                                    }
                                }
                            }
                        }
                        Signal::Pause => {} // Nothing should done, partially completed files should remain
                    }
                    break;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    todo!()
                }
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
                Ok(StatusCode::OK)
            }
            Err(err) => {
                error!(
                    "download with url: {}, failed with err: {}",
                    options.url, err
                );
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    async fn add_download_handler(&self, url: Url, tx: Sender<Signal>) {
        self.downloads
            .lock()
            .await
            .insert(url, (Status::Running, Some(tx)));
    }

    pub async fn cancel_download(&self, url: Url) -> Result<Status> {
        match self.downloads.lock().await.get(&url) {
            Some(download) => match download {
                (Status::Running, Some(tx)) => match tx.send(Signal::Cancel).await {
                    Ok(_) => Ok(Status::Canceled),
                    Err(_) => Err(Error::FailedToHalt),
                },
                _ => Err(Error::NotDownloading),
            },
            None => Err(Error::NotDownloading),
        }
    }

    pub async fn pause_download(&self, url: Url) -> Result<Status> {
        match self.downloads.lock().await.get(&url) {
            Some(download) => match download {
                (Status::Running, Some(tx)) => match tx.send(Signal::Pause).await {
                    Ok(_) => Ok(Status::Paused),
                    Err(_) => Err(Error::FailedToHalt),
                },
                _ => Err(Error::NotDownloading),
            },
            None => Err(Error::NotDownloading),
        }
    }

    async fn check_url_availability(
        &self,
        options: &DownloadOptions,
    ) -> std::result::Result<ExitStatus, std::io::Error> {
        info!("{}, {}", &options.url.as_str(), YTDLP_LOCATION);
        Command::new(YTDLP_LOCATION)
            .arg("--simulate")
            .arg(&options.url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .await
    }

    async fn get_filename(&self, options: &DownloadOptions) -> Option<String> {
        let child = Command::new(YTDLP_LOCATION)
            .arg("-o")
            .arg("%(title)s")
            .arg("--get-filename")
            .arg(&options.url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn();

        match child {
            Ok(mut child) => {
                let stderr = child.stdout.take().unwrap();
                let mut reader = BufReader::new(stderr).lines();
                match child.wait().await {
                    Ok(status) => match status.success() {
                        true => {
                            let mut last_line = String::new();
                            while let Ok(Some(line)) = reader.next_line().await {
                                last_line = line;
                            }
                            Some(last_line)
                        }
                        false => None,
                    },
                    Err(_) => None,
                }
            }
            Err(_) => todo!(),
        }
    }
}

#[derive(sqlx::Type, Debug, Serialize, Deserialize)]
struct Download {
    id: i64,
    status: Status,
}


async fn init_from_db(db: SqlitePool) -> Arc<Mutex<HashMap<Url, (Status, Option<Sender<Signal>>)>>> {
    let downloads = sqlx::query_as!(Download, "SELECT * FROM Download")
            .fetch_all(&db)
            .await;
    // Map downloads to map.

    Arc::new(Mutex::new(HashMap::new()))
}

pub async fn routes(db: SqlitePool, download_path: PathBuf) -> Router {
    // let (tx, rx) = broadcast::channel(100);
    Router::new()
        .route("/", post(download_from_options))
        .route("/cancel", post(cancel_download))
        .route("/check", post(check_url_availability))
        .route("/pause", post(pause_download))
        .with_state(YtdlpClient::new(db, download_path).await)
}

async fn download_from_options(
    State(ytdlp_client): State<YtdlpClient>,
    Json(options): Json<DownloadOptions>,
) -> StatusCode {
    match ytdlp_client.download_from_options(&options).await {
        Ok(_) => todo!(),
        Err(_) => todo!(),
    }
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

#[axum::debug_handler]
async fn check_url_availability(
    State(ytdlp_client): State<YtdlpClient>,
    Json(options): Json<DownloadOptions>,
) -> StatusCode {
    match ytdlp_client.check_url_availability(&options).await {
        Ok(status) => {
            info!(
                "download status for url: {}, {}",
                options.url.as_str(),
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
