export interface DownloadOptions {
    container: string;
    name_format: string;
    quality: string;
}

export interface DownloadEntry {
    options: DownloadOptions;
    status: string;
}