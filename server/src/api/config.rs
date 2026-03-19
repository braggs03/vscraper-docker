use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use tracing::error;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
struct Config {
    id: i64,
    skip_homepage: bool,
}

pub fn routes(db: SqlitePool) -> Router {
    Router::new()
        .route("/", get(get_config))
        .route("/homepage/{preference}", post(set_skip_homepage))
        .with_state(db)
}

async fn get_config(State(db): State<SqlitePool>) -> Result<Json<Value>, StatusCode> {
    let cfg = sqlx::query_as!(Config, "SELECT * FROM Config WHERE id = 1")
        .fetch_one(&db)
        .await;

    match cfg {
        Ok(cfg) => Ok(Json(serde_json::json!(cfg))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn set_skip_homepage(
    State(db): State<SqlitePool>,
    Path(preference): Path<bool>,
) -> Result<StatusCode, StatusCode> {
    let status = sqlx::query_as!(
        Config,
        "UPDATE Config SET skip_homepage = $1 WHERE id=1",
        preference
    )
    .execute(&db)
    .await;

    match status {
        Ok(result) => match result.rows_affected() {
            1 => Ok(StatusCode::OK),
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(err) => {
            error!("failed to set skip_homepage: {err}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        },
    }
}
