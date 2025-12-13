use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Clone, Debug, Serialize)]
struct Config {
    id: Option<i64>,
    skip_homepage: Option<bool>,
}

pub fn routes(db: SqlitePool) -> Router {
    Router::new()
        .route("/", get(get_config))
        .route("/{preference}", post(set_skip_homepage))
        .with_state(db)
}

async fn get_config(State(db): State<SqlitePool>) -> Result<Json<Value>, StatusCode> {
    let cfg = sqlx::query_as_unchecked!(Config, "SELECT * FROM Config WHERE id = 1")
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
    let status = sqlx::query("UPDATE Config SET skip_homepage = $1 WHERE id=1")
        .bind(preference)
        .execute(&db)
        .await;

    match status {
        Ok(result) => match result.rows_affected() {
            1 => Ok(StatusCode::OK),
            _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
        },
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
