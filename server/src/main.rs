use axum::{
    http::{HeaderName, Method},
    Router,
};
use serde::Deserialize;
use sqlx::{
    sqlite::SqliteConnectOptions,
    SqlitePool,
};
use std::{io::Error, str::FromStr};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::Level;

mod api;
mod error;
mod ytdlp;

#[derive(Deserialize, Debug)]
struct Args {
    #[serde(default = "default_db_url")]
    db_url: String,
    #[serde(default = "default_download_location")]
    download_location: String,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default = "default_ytdlp_path")]
    ytdlp_path: String,
}

fn default_db_url() -> String {
    String::from("sqlite://sqlite.db")
}

fn default_download_location() -> String {
    String::from("/downloads/")
}

fn default_log_level() -> String {
    String::from("info")
}

fn default_ytdlp_path() -> String {
    String::from("yt-dlp")
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenv::dotenv();

    let args = match envy::from_env::<Args>() {
        Ok(config) => config,
        Err(error) => panic!("{:#?}", error),
    };

    tracing_subscriber::fmt()
        .with_max_level(
            Level::from_str(&args.log_level).expect("couldn't pass log_level to known level"),
        )
        .init();

    let options = SqliteConnectOptions::from_str(&args.db_url)
        .unwrap()
        .create_if_missing(true);
    let db = SqlitePool::connect_with(options)
        .await
        .expect("could create/connect with to the sqlite database.");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("failed to run migrations on db.");
    create_default_config(&db).await;

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(Any)
        .allow_headers([HeaderName::from_static("content-type")]);
    let static_dir = ServeDir::new("static");
    let app = Router::new()
        .nest(
            "/api",
            api::routes(db, args.ytdlp_path, args.download_location.into()).await,
        )
        .fallback_service(static_dir)
        .layer(cors);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn create_default_config(db: &SqlitePool) {
    match sqlx::query!(
        r#"INSERT INTO Config (
            id,
            skip_homepage
        )
        VALUES (
            1, 
            false
        )
        ON CONFLICT(id) DO NOTHING"#,
    )
    .execute(db)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            panic!("failed to create default config: {}", err);
        }
    }
}
