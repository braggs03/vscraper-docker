use axum::Router;
use serde::Deserialize;
use server::create_default_config;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::{io::Error, str::FromStr};
use tower_http::services::ServeDir;
use tracing::Level;

mod api;
mod core;
mod error;

// <----- Args - Environmental Variables ----->

#[derive(Deserialize, Debug)]
struct Args {
    #[serde(default = "default_database_location")]
    database_url: String,
    #[serde(default = "default_download_location")]
    download_location: String,
    #[serde(default = "default_log_level")]
    log_level: String,
    /// Comma separated addresses
    // _origins: String,
    #[serde(default = "default_ytdlp_path")]
    ytdlp_path: String,
}

fn default_database_location() -> String {
    String::from("sqlite://sqlite")
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

// <----- Main ----->

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenv::dotenv();

    let args = match envy::from_env::<Args>() {
        Ok(config) => config,
        Err(error) => panic!("{:#?}", error),
    };

    tracing_subscriber::fmt()
        .with_max_level(
            Level::from_str(&args.log_level).expect("couldn't convert log_level to known level"),
        )
        .init();

    let options = SqliteConnectOptions::from_str(&args.database_url)
        .unwrap()
        .create_if_missing(true);

    let db = SqlitePool::connect_with(options)
        .await
        .expect("could not create/connect the sqlite database.");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("failed to run migrations on db.");
    create_default_config(&db).await;

    // let cors = CorsLayer::new()
    //     .allow_methods([Method::GET, Method::POST])
    //     .allow_origin(
    //         // args.origins
    //         //     .split(",")
    //         //     .map(|origin| {
    //         //         origin
    //         //             .parse::<HeaderValue>()
    //         //             .expect("origin could not be parsed.")
    //         //     })
    //         //     .collect::<Vec<_>>(),
    //         Any
    //     )
    //     .allow_headers([HeaderName::from_static("content-type")]);

    let app = Router::new()
        .nest(
            "/api",
            api::routes(db, args.ytdlp_path, args.download_location.into()).await,
        )
        .fallback_service(ServeDir::new("static"));
        // .layer(cors);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?; // TODO - .with_graceful_shutdown

    Ok(())
}
