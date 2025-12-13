use std::{env, fs, path::PathBuf};

use axum::{http::Method, Router};
use error::Error;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::{error, info, instrument::WithSubscriber, Level};

mod api;
mod error;

const DB_URL_DEFAULT: &str = "sqlite://sqlite.db";
const DB_URL_KEY: &str = "DB_URL";
const DOWNLOAD_LOCATION: &str = "DOWNLOAD_LOCATION";
const DOWNLOAD_LOCATION_DEFAULT: &str = "/downloads";
const LOG_LEVEL_DEFAULT: Level = Level::INFO;
const LOG_LEVEL_KEY: &str = "LOG_LEVEL";

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenv::dotenv();

    let db_url = env::var(DB_URL_KEY).unwrap_or(String::from(DB_URL_DEFAULT));
    let download_location = PathBuf::from(
        env::var(DOWNLOAD_LOCATION).unwrap_or(String::from(DOWNLOAD_LOCATION_DEFAULT)),
    );
    let level = if let Ok(level) = env::var(LOG_LEVEL_KEY) {
        str_to_log_level(&level)
    } else {
        LOG_LEVEL_DEFAULT
    };

    tracing_subscriber::fmt().with_max_level(level).init();

    let db = init_db(&db_url).await;

    let cors = cors();

    let static_dir = ServeDir::new("static");
    let app = Router::new()
        .nest("/api", api::routes(db, download_location))
        .fallback_service(static_dir)
        .layer(cors);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_origin(Any)
}

fn str_to_log_level(level: &str) -> Level {
    match level {
        "Trace" | "trace" => Level::TRACE,
        "Debug" | "debug" => Level::DEBUG,
        "Info" | "info" => Level::INFO,
        "Warn" | "warn" => Level::WARN,
        "Error" | "error" => Level::ERROR,
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
    let pool = SqlitePool::connect(db_url).await.unwrap();
    create_tables(pool.clone()).await;

    pool
}

async fn create_tables(db: SqlitePool) {
    let init_sql = fs::read_to_string("./data/init.sql").expect("Failed to read init.sql");
    match sqlx::query(&init_sql).execute(&db).await {
        Ok(_) => {}
        Err(err) => error!("couldn't create base tables: {}", err),
    }

    match sqlx::query(
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
    .execute(&db)
    .await
    {
        Ok(_) => {}
        Err(err) => {
            error!("failed to create default config: {}", err);
        }
    }
}
