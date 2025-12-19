use std::path::PathBuf;

use axum::Router;
use sqlx::SqlitePool;

mod config;
mod ytdlp;

pub async fn routes(db: SqlitePool, ytdlp_path: String, download_path: PathBuf) -> Router {
    Router::new()
        .nest("/config", config::routes(db.clone()))
        .nest("/download", ytdlp::routes(db, ytdlp_path, download_path).await)
}
