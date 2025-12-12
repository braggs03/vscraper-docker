use axum::{Router, routing::get};

mod ytdlp;
mod config;
mod appdata;

pub fn routes() -> Router {
    Router::new().route("/test", get(check_health))
}

async fn check_health() -> String {
    "{\"data\":\"bug\"}".to_string()
}