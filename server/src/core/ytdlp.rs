use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use strum_macros::EnumString;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::{error::TryRecvError, Receiver, Sender};
use tracing::{debug, error, info, trace};
use ts_rs::TS;
use url::Url;

// <----- Constants ----->

const YTDLP_DOWNLOAD_UPDATE_REGEX: &str = r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?\s+?(\d+(?:\.\d+)?[GMK]iB)\s+at\s+(\d+\.\d+(?:[GMK]i)?B\/s)\s+ETA\s+((\d+:\d+)|(?:Unknown))";

// <----- Types ----->

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    DownloadAlreadyPresent { status: Status },
    FailedCheck,
    FailedToHalt,
    NotDownloading,
    General { err: std::io::Error },
}

#[derive(Clone)]
pub struct YtdlpClient {
    download_path: PathBuf,
    downloads: Arc<DashMap<Url, Download>>,
    pub ytdlp_path: String,
}

#[derive(Clone, Serialize, TS)]
#[ts(export)]
pub struct Download {
    #[serde(skip)]
    download_termination: Option<Sender<Signal>>,
    options: DownloadOptions,
    progress: DownloadProgress,
    status: Status,
    title: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct DownloadOptions {
    container: String,
    name_format: String,
    quality: String,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DownloadUpdate {
    url: Url,
    progress: DownloadProgress,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct DownloadProgress {
    percent: String,
    size_downloaded: String,
    speed: String,
    eta: String,
}

#[derive(Clone, Debug, Deserialize, EnumString, Serialize, sqlx::Type, PartialEq, TS)]
#[ts(export)]
#[sqlx(type_name = "status")]
pub enum Status {
    Canceled,
    Completed,
    Failed,
    Paused,
    Running,
}

#[derive(Clone, Deserialize, PartialEq)]
pub enum Signal {
    Cancel,
    Pause,
}

// <----- Impl ----->

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::General { err }
    }
}

impl From<&Signal> for Status {
    fn from(value: &Signal) -> Self {
        match value {
            Signal::Cancel => Status::Canceled,
            Signal::Pause => Status::Paused,
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl Default for DownloadProgress {
    fn default() -> Self {
        DownloadProgress {
            percent: String::from("0.0"),
            size_downloaded: String::from("0.0KiB"),
            speed: String::from("0.0KiB/s"),
            eta: String::from("00:00"),
        }
    }
}

impl YtdlpClient {
    pub async fn new(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> YtdlpClient {
        YtdlpClient {
            download_path,
            downloads: init_from_db(db).await,
            ytdlp_path,
        }
    }

    pub async fn add_download(
        &self,
        url: &Url,
        options: &DownloadOptions,
        tx: Option<Sender<Signal>>,
    ) -> Result<()> {
        match self.downloads.remove(&url) {
            Some((url, download)) => match download.status {
                Status::Canceled | Status::Failed | Status::Paused => {
                    self.downloads.insert(
                        url.clone(),
                        Download {
                            download_termination: tx,
                            options: options.clone(),
                            progress: download.progress, // FIX - Shouldn't be set to old progress (probably)
                            status: Status::Running,
                            title: download.title,
                        },
                    );

                    Ok(())
                }
                Status::Completed | Status::Running => {
                    // TODO - Compare options - if different, should a new download be added?
                    self.downloads.insert(
                        url,
                        Download {
                            status: download.status.clone(),
                            ..download
                        },
                    );
                    Err(Error::DownloadAlreadyPresent {
                        status: download.status,
                    })
                }
            },
            None => {
                self.downloads.insert(
                    url.clone(),
                    Download {
                        options: options.clone(),
                        progress: DownloadProgress::default(),
                        status: Status::Running,
                        download_termination: tx,
                        title: self.get_title(&url).await?,
                    },
                );

                Ok(())
            }
        }
    }

    pub async fn check_url_availability(&self, url: &Url, options: &DownloadOptions) -> Result<()> {
        debug!(
            "running check for url: {}, with options: {:?}",
            url, options
        );

        let mut child = Command::new(&self.ytdlp_path)
            .arg("--remote-components")
            .arg("ejs:github")
            .arg("--simulate")
            .arg("-o")
            .arg(&options.name_format)
            .arg("-f")
            .arg(self.get_format(&options))
            .arg(url.as_str())
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;

        let stderr = child.stderr.take().unwrap();
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            debug!("ytdlp check error: {}", line);
        }

        if child.wait().await?.success() {
            Ok(())
        } else {
            Err(Error::FailedCheck)
        }
    }

    pub async fn download(
        &self,
        url: &Url,
        options: &DownloadOptions,
        mut download_kill_rx: Receiver<Signal>,
        download_update_tx: Option<Sender<DownloadUpdate>>,
    ) -> Result<Status> {
        let mut received_signal = None;
        let download_path = self.download_path.clone().join(&options.name_format);

        debug!("downloading from url");
        let mut child = Command::new(&self.ytdlp_path)
            .arg("--remote-components")
            .arg("ejs:github")
            .arg("--newline")
            .arg("-f")
            .arg(self.get_format(options))
            .arg("--merge-output-format")
            .arg(&options.container)
            .arg("--rate-limit")
            .arg("100K")
            .arg("-o")
            .arg(download_path)
            .arg(url.as_str())
            .stderr(Stdio::inherit())
            .stdout(Stdio::piped())
            .spawn()?;

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

            if let Some(signal) = self
                .handle_kill_download(url, &mut child, &mut download_kill_rx)
                .await
            {
                received_signal = Some(signal);
                break;
            }

            if regex.is_match(&line) {
                if let Some(captures) = regex.captures(&line) {
                    let url = url.clone();
                    let percent = String::from(&captures[1]);
                    let size_downloaded = String::from(&captures[2]);
                    let speed = String::from(&captures[3]);
                    let eta = String::from(&captures[4]);

                    let download_progress = DownloadProgress {
                        percent,
                        size_downloaded,
                        speed,
                        eta,
                    };

                    match self.downloads.get_mut(&url) {
                        Some(mut download) => download.progress = download_progress.clone(),
                        None => error!("tried to update download progress, no url for download value found in downloads."),
                    }

                    let download_update = DownloadUpdate {
                        url,
                        progress: download_progress,
                    };

                    trace!("download update: {:?}", download_update);
                    if let Some(ref download_update_tx) = download_update_tx {
                        let send_result = download_update_tx.send(download_update).await;
                        server::handle_send(send_result);
                    }
                }
            }
        }

        match child.wait().await?.success() {
            true => Ok(Status::Completed),
            false => match received_signal {
                Some(signal) => match signal {
                    Signal::Cancel => Ok(Status::Canceled),
                    Signal::Pause => Ok(Status::Paused),
                },
                None => Ok(Status::Failed),
            },
        }
    }

    pub fn get_downloads(&self) -> DashMap<Url, Download> {
        (*self.downloads).clone()
    }

    fn get_format(&self, options: &DownloadOptions) -> String {
        format!("bestvideo[height={}]+bestaudio/best", &options.quality)
    }

    pub async fn get_present_urls(&self) -> Result<Vec<Url>> {
        Ok(self
            .downloads
            .iter()
            .map(|entry| entry.key().clone())
            .collect())
    }

    async fn get_title(&self, url: &Url) -> Result<String> {
        let output = Command::new(&self.ytdlp_path)
            .arg("--simulate")
            .arg("--get-title")
            .arg(url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .output()
            .await?;

        let mut stdio_lines = output.stdout.lines();
        let mut last_line = String::from("unknown");
        while let Ok(Some(line)) = stdio_lines.next_line().await {
            last_line = line;
        }

        Ok(last_line)
    }

    async fn handle_kill_download(
        &self,
        url: &Url,
        child: &mut Child,
        download_kill_rx: &mut Receiver<Signal>,
    ) -> Option<Signal> {
        match download_kill_rx.try_recv() {
            Ok(signal) => {
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

                                if signal == Signal::Cancel {
                                    debug!("signal cancel received, removing partially downloaded files. NOT YET IMPLEMENTED");
                                    // TODO - FIX THIS
                                }
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
                Some(signal)
            }
            Err(TryRecvError::Disconnected) => None, //TODO - What should this do? Kill download? Unreachable?
            Err(TryRecvError::Empty) => None,
        }
    }

    pub async fn interrupt_download(&self, url: &Url, signal: &Signal) -> Result<Status> {
        match self.downloads.remove(&url) {
            Some((url, download)) => match download {
                Download {
                    download_termination: Some(tx),
                    options,
                    progress,
                    status: Status::Running,
                    title,
                } => match tx.send(signal.clone()).await {
                    Ok(_) => {
                        self.downloads.insert(
                            url,
                            Download {
                                download_termination: None,
                                options,
                                progress,
                                status: Status::from(signal),
                                title,
                            },
                        );
                        Ok(Status::Paused)
                    }
                    Err(_) => Err(Error::FailedToHalt),
                },
                _ => Err(Error::NotDownloading), // TODO - FIX THIS - SHOULD BE A DIFFERENT
            },
            None => Err(Error::NotDownloading),
        }
    }

    /// Used to grab all urls - i.e. the requested link is a playlist.
    pub async fn get_all_urls(&self, url: &Url) -> Result<Vec<Url>> {
        let output = Command::new(&self.ytdlp_path)
            .arg("--flat-playlist")
            .arg("--print")
            .arg("%(url)s")
            .arg(url.as_str())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .output()
            .await?;

        let mut stdio_lines = output.stdout.lines();
        let mut urls = Vec::new();
        while let Ok(Some(line)) = stdio_lines.next_line().await {
            if let Ok(url) = Url::parse(&line) {
                debug!("found url: {}", url);
                urls.push(url);
            }
        }

        Ok(urls)
    }

    // async fn remove_partial_files(&self, url: &Url, options: &DownloadOptions) {
    //     if let Some(download_file_name) = self.get_filename(url, options).await {
    //         for dir in std::fs::read_dir(&self.download_path) {
    //             for file in dir {
    //                 match file {
    //                     Ok(file) => match file.file_name().into_string() {
    //                         Ok(file_name) => {
    //                             if file_name.contains(&download_file_name) {
    //                                 info!(
    //                                     "removing file: {}",
    //                                     file.file_name()
    //                                         .into_string()
    //                                         .unwrap_or("unknown".to_string())
    //                                 );
    //                                 let _ = fs::remove_file(file.path());
    //                             }
    //                         }
    //                         Err(_) => todo!(),
    //                     },
    //                     Err(_) => todo!(),
    //                 }
    //             }
    //         }
    //     }
    // }

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

// <----- Functions ----->

async fn init_from_db(_db: SqlitePool) -> Arc<DashMap<Url, Download>> {
    // let rows = sqlx::query!("SELECT * FROM Download").fetch_all(&db).await;
    // let downloads = match rows {
    //     Ok(rows) => {
    //         let downloads: Vec<(Url, Status, DownloadOptions)> = rows
    //             .into_iter()
    //             .map(|row| {
    //                 let url = Url::parse(&row.url).expect("Failed to parse URL");
    //                 let status = Status::from_str(&row.status).unwrap_or({
    //                     error!("failed to parse Status from db, defaulting to Status::Failed");
    //                     Status::Failed
    //                 });
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
    //     .map(|x| (x.0, Download { status: x.1, options: x.2, download_termination: None }))
    //     .collect::<DashMap<_, _>>();

    let download_map = DashMap::new();

    Arc::new(download_map)
}
