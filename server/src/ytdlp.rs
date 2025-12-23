use axum::Json;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::Sender;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{debug, error, info, trace};
use url::Url;

const YTDLP_DOWNLOAD_UPDATE_REGEX: &str = r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?\s+?(\d+(?:\.\d+)?[GMK]iB)\s+at\s+(\d+\.\d+(?:[GMK]i)?B\/s)\s+ETA\s+((\d+:\d+)|(?:Unknown))";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    FailedToComplete,
    FailedToHalt,
    FailedToStart,
    NotDownloading,
    DownloadAlreadyPresent,
}

#[derive(Clone)]
pub struct YtdlpClient {
    db: SqlitePool,
    download_path: PathBuf,
    pub downloads: Arc<Mutex<HashMap<Url, (Status, DownloadOptions, Option<Sender<Signal>>)>>>,
    ytdlp_path: String,
}

#[derive(Clone, Debug, Deserialize, FromRow, Serialize)]
pub struct DownloadOptions {
    pub container: String,
    pub name_format: String,
    pub quality: String,
}

#[derive(Serialize)]
struct DownloadProgress {
    url: Url,
    percent: String,
    size_downloaded: String,
    speed: String,
    eta: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[sqlx(type_name = "status")]
pub enum Status {
    Canceled,
    Completed,
    Failed,
    None,
    Paused,
    Running,
}

#[derive(Clone)]
pub enum Signal {
    Cancel,
    Pause,
}

impl From<String> for Status {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Canceled" => Status::Canceled,
            "Completed" => Status::Completed,
            "None" => Status::None,
            "Paused" => Status::Paused,
            "Running" => Status::Running,
            _ => panic!("Wrong value in db."),
        }
    }
}

async fn init_from_db(
    db: SqlitePool,
) -> Arc<Mutex<HashMap<Url, (Status, DownloadOptions, Option<Sender<Signal>>)>>> {
    // let rows = sqlx::query!("SELECT * FROM Download").fetch_all(&db).await;
    // let downloads = match rows {
    //     Ok(rows) => {
    //         let downloads: Vec<(Url, Status, DownloadOptions)> = rows
    //             .into_iter()
    //             .map(|row| {
    //                 let url = Url::parse(&row.url).expect("Failed to parse URL");
    //                 let status = Status::from(row.status);
    //                 (
    //                     url,
    //                     status,
    //                     DownloadOptions {
    //                         container: row.container,
    //                         name_format: row.name_format,
    //                         quality: row.quality,
    //                     },
    //                 )
    //             })
    //             .collect();

    //         downloads
    //     }
    //     Err(_) => todo!(),
    // };

    // let download_map = downloads
    //     .into_iter()
    //     .map(|x| (x.0, (x.1, x.2, None)))
    //     .collect::<HashMap<_, (_, _, _)>>();
    // Arc::new(Mutex::new(download_map))
    Arc::new(Mutex::new(HashMap::new()))
}

impl YtdlpClient {
    pub async fn new(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> YtdlpClient {
        YtdlpClient {
            db: db.clone(),
            download_path,
            downloads: init_from_db(db).await,
            ytdlp_path,
        }
    }

    pub async fn download_from_options(
        &self,
        url: &Url,
        options: &DownloadOptions,
        download_update_tx: Option<Sender<String>>,
    ) -> Result<Status> {
        let mut received_signal = None;
        let download_path = self.download_path.clone().join(&options.name_format);
        let (download_kill_tx, mut download_kill_rx) = mpsc::channel(100);
        self.downloads
            .lock()
            .await
            .insert(url.clone(), (Status::Running, options.clone(), Some(download_kill_tx)));

        debug!("checking url availability for: {}", url);
        match self.check_url_availability(&url, &options).await {
            Ok(exit_status) => {
                if exit_status.success() {
                } else {
                    // TODO: Parse stderr to provide exact error caused by yt-dlp.
                    // Return generic error in place of other errors
                    return Err(Error::FailedToStart);
                }
                // WEBSOCKET: Emission::YtdlpUrlUpdate
            }
            Err(err) => match err.kind() {
                err => error!("executing command: {}", err),
            },
        }

        debug!("downloading from url");
        let mut child = Command::new(&self.ytdlp_path)
            .arg("--newline")
            .arg("--rate-limit")
            .arg("100K")
            .arg("-o")
            .arg(download_path)
            .arg(url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        debug!(
            "spawned ytdlp download from url: {}, with pid: {}",
            url,
            child
                .id()
                .map_or("unknown".to_string(), |code| code.to_string())
        );

        let stderr = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stderr).lines();
        let regex = Regex::new(YTDLP_DOWNLOAD_UPDATE_REGEX).expect("couldn't compile yt-dlp regex");

        while let Ok(Some(line)) = reader.next_line().await {
            trace!("ytdlp output: {}", line);
            match download_kill_rx.try_recv() {
                Ok(signal) => {
                    received_signal = Some(signal.clone());
                    let pid = child
                        .id()
                        .map_or("unknown".to_string(), |code| code.to_string());
                    debug!("received kill signal for url: {}, pid: {}", url, pid);
                    match child.kill().await {
                        Ok(_) => {
                            info!("successfully killed child for url: {}, pid: {}", url, pid);
                            match child.wait().await {
                                Ok(exit_status) => {
                                    debug!(
                                        "killed zombie child for url: {}, pid: {}, exit code: {}",
                                        url, pid, exit_status
                                    );
                                }
                                Err(err) => {
                                    error!(
                                        "failed to kill zombie child for url: {}, pid: {}, err: {}",
                                        url, pid, err
                                    );
                                }
                            }
                        }
                        Err(err) => error!(
                            "failed to kill child for url: {}, pid: {} err: {}",
                            url, pid, err
                        ),
                    }

                    match signal {
                        Signal::Cancel => {
                            let download_file_name = self.get_filename(&url, options).await;
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
                Err(TryRecvError::Disconnected) => {}
            }
            if regex.is_match(&line) {
                if let Some(captures) = regex.captures(&line) {
                    let url = url.clone();
                    let percent = String::from(&captures[1]);
                    let size_downloaded = String::from(&captures[2]);
                    let speed = String::from(&captures[3]);
                    let eta = String::from(&captures[4]);

                    let json = serde_json::json!({
                        "url":url,
                        "percent":percent,
                        "size_downloaded":size_downloaded,
                        "speed":speed,
                        "eta":eta
                    });

                    if let Some(ref download_update_tx) = download_update_tx {
                        download_update_tx.send(json.to_string()).await;
                    }
                }
            }
        }

        let status: Status = match child.wait().await {
            Ok(status) => match status.success() {
                true => Status::Completed,
                false => match received_signal {
                    Some(signal) => match signal {
                        Signal::Cancel => Status::Canceled,
                        Signal::Pause => Status::Paused,
                    },
                    None => Status::Failed,
                },
            },
            Err(_) => Status::Failed,
        };

        todo!()
    }

    // async fn add_download_handler(
    //     &self,
    //     url: &Url,
    //     options: &DownloadOptions,
    //     tx: Sender<Signal>,
    // ) -> Result<()> {
    //     if self.downloads.lock().await.contains_key(url) {
    //         return Err(Error::DownloadAlreadyPresent);
    //     }

    //     self.downloads
    //         .lock()
    //         .await
    //         .insert(url.clone(), (Status::Running, options.clone(), Some(tx)));

    //     match self.insert_download_db(url, Status::Running, options).await {
    //         Ok(_) => info!("download with url successfully added to database: {}", url),
    //         Err(err) => return Err(err),
    //     }

    //     Ok(())
    // }

    pub async fn cancel_download(&self, url: Url) -> Result<Status> {
        let mut downloads = self.downloads.lock().await;
        match downloads.remove(&url) {
            Some(download) => match download {
                (Status::Running, options, Some(tx)) => match tx.send(Signal::Cancel).await {
                    Ok(_) => {
                        downloads.insert(url, (Status::Canceled, options.clone(), None));
                        Ok(Status::Canceled)
                    }
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
                (Status::Running, _, Some(tx)) => match tx.send(Signal::Pause).await {
                    Ok(_) => Ok(Status::Paused),
                    Err(_) => Err(Error::FailedToHalt),
                },
                _ => Err(Error::NotDownloading),
            },
            None => Err(Error::NotDownloading),
        }
    }

    pub async fn check_url_availability(
        &self,
        url: &Url,
        options: &DownloadOptions,
    ) -> std::result::Result<ExitStatus, std::io::Error> {
        info!("ytdlp path: {}", self.ytdlp_path);
        Command::new(&self.ytdlp_path)
            .arg("--simulate")
            .arg(url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .await
    }

    async fn get_filename(&self, url: &Url, options: &DownloadOptions) -> Option<String> {
        let child = Command::new(&self.ytdlp_path)
            .arg("-o")
            .arg("%(title)s")
            .arg("--get-filename")
            .arg(url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .output()
            .await;

        if let Ok(output) = child {
            if output.status.success() {
                let mut last_line = String::new();
                let mut lines = output.stdout.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    last_line = line;
                }
                return Some(last_line);
            }
        };

        None
    }

    // async fn insert_download_db(
    //     &self,
    //     url: &Url,
    //     status: Status,
    //     options: &DownloadOptions,
    // ) -> Result<()> {
    //     match sqlx::query(
    //         r#"INSERT INTO Download (
    //         url,
    //         status,
    //         container,
    //         name_format,
    //         quality
    //     )
    //     VALUES (
    //         $1,
    //         $2,
    //         $3,
    //         $4,
    //         $5
    //     )
    //     ON CONFLICT(url) DO NOTHING"#,
    //     )
    //     .bind(url.as_str())
    //     .bind(status)
    //     .bind(options.container.clone())
    //     .bind(options.name_format.clone())
    //     .bind(options.quality.clone())
    //     .execute(&self.db)
    //     .await
    //     {
    //         Ok(query) => match query.rows_affected() {
    //             1 => Ok(()),
    //             0 => Err(Error::DownloadAlreadyPresent),
    //             _ => panic!("tried to edit/insert multiple downloads"),
    //         },
    //         Err(err) => {
    //             panic!("failed to create default config: {}", err);
    //         }
    //     }
    // }
}
