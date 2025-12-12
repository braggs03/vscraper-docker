export interface Config {
    skip_homepage: boolean,
}

export interface DownloadProgress {
    url: string,
    percent: string,
    size_downloaded: string,
    speed: string,
    eta: string,
}

export enum Emission {
    FfmpegInstall = "ffmpeg_install",
    YtdlpDownloadUpdate = "ytdlp_download_update",
    YtdlpInstall = "ytdlp_install",
    YtdlpUrlSuccess = "ytdlp_url_success",
}