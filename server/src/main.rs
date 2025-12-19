use axum::{
    http::{HeaderName, Method},
    Router,
};
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use std::{env, io::Error, path::PathBuf};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::{error, info, Level};

mod api;
mod error;
mod ytdlp;

/// Default name of sqlite database.
const DB_URL_DEFAULT: &str = "sqlite://sqlite.db";
/// Env to grab for sql database, if not present, will default to DB_URL_DEFAULT.
const DB_URL_KEY: &str = "DB_URL";
/// Env to grab for download location.
const DOWNLOAD_LOCATION: &str = "DOWNLOAD_LOCATION";
/// Env to grab for ytdlp location, if not present, will default to YTDLP_LOCATION_DEFAULT.
const YTDLP_LOCATION: &str = "YTDLP_LOCATION";
/// Use application from path.
const YTDLP_LOCATION_DEFAULT: &str = "yt-dlp";
/// Default log level for application.
const LOG_LEVEL_DEFAULT: Level = Level::INFO;
/// Env to grab for log level, if not present, will default to LOG_LEVEL_DEFAULT.
const LOG_LEVEL_KEY: &str = "LOG_LEVEL";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenv::dotenv();

    let db_url = env::var(DB_URL_KEY).unwrap_or(String::from(DB_URL_DEFAULT));
    let download_location = PathBuf::from(env::var(DOWNLOAD_LOCATION).expect(&format!(
        "set {} to preferred download location.",
        DOWNLOAD_LOCATION
    )));
    let level = if let Ok(level) = env::var(LOG_LEVEL_KEY) {
        str_to_log_level(&level.to_ascii_lowercase())
    } else {
        LOG_LEVEL_DEFAULT
    };
    let ytdlp_path = env::var(YTDLP_LOCATION).unwrap_or(String::from(YTDLP_LOCATION_DEFAULT));

    tracing_subscriber::fmt().with_max_level(level).init();
    let db = init_db(&db_url).await;
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(Any)
        .allow_headers([HeaderName::from_static("content-type")]);
    let static_dir = ServeDir::new("static");

    let app = Router::new()
        .nest("/api", api::routes(db, ytdlp_path, download_location).await)
        .fallback_service(static_dir)
        .layer(cors);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn str_to_log_level(level: &str) -> Level {
    match level {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => panic!("unknown log level"),
    }
}

async fn init_db(db_url: &str) -> SqlitePool {
    match Sqlite::database_exists(db_url).await {
        Ok(exist) => {
            if exist {
                info!("db exist, skipping creation");
            } else {
                info!("db not found, creating.");
                match Sqlite::create_database(db_url).await {
                    Ok(_) => info!("created db "),
                    Err(error) => {
                        error!("creating db: {}, exiting", error);
                        std::process::exit(1);
                    }
                }
            }
        }
        Err(err) => error!("failed retrieving database: {}", err),
    }
    let db = SqlitePool::connect(db_url).await.unwrap();

    create_default_config(&db).await;

    db
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
