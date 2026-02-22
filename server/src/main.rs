use axum::Router;
use regex::Replacer;
use serde::Deserialize;
use server::create_default_config;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::{io::Error, path::{Path, PathBuf}, str::FromStr};
use tower_http::services::ServeDir;
use tracing::Level;

mod api;
mod core;
mod error;

// <----- Args - Environmental Variables ----->

#[derive(Deserialize, Debug)]
struct Args {
    #[serde(default = "default_database_name")]
    database_name: String,
    database_url: String,
    download_location: String,
    #[serde(default = "default_log_level")]
    log_level: String,
    #[serde(default = "default_ytdlp_path")]
    ytdlp_path: String,
}

fn default_database_name() -> String {
    String::from("sqlite.db")
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
    #[cfg(debug_assertions)]
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

    // let db_name = if args.database_url.ends_with("/") { &args.database_name } else { &format!("/{}", &args.database_name) };

    // let database_real_path: PathBuf = ["sqlite://", &args.database_url, db_name].iter().collect();

    // println!("{}", database_real_path.to_str().unwrap());

    // let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", &args.database_url))
    
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
