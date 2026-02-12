use tracing::error;

pub async fn create_default_config(db: &sqlx::SqlitePool) {
    if let Err(err) = sqlx::query!(
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
        panic!("failed to create default config: {}", err);
    }
}

pub fn handle_send<T: std::fmt::Display>(send_result: Result<(), T>) {
    if let Err(err) = send_result {
        error!("send error: {}", err);
    }
}
