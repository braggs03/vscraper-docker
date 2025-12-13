use std::path::PathBuf;

use axum::Router;
use sqlx::SqlitePool;

mod config;
mod ytdlp;

pub fn routes(db: SqlitePool, download_path: PathBuf) -> Router {
    Router::new()
        .nest("/config", config::routes(db.clone()))
        .nest("/download", ytdlp::routes(db, download_path))
}
