use tracing::{error, trace};

pub async fn create_default_config(db: &sqlx::SqlitePool) {
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

pub fn handle_send<T: std::fmt::Display>(send_result: Result<(), T>) {
    match send_result {
        Ok(_) => trace!("successful send to client."),
        Err(err) => error!("send error: {}", err),
    }
}
