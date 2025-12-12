use axum::{Router, http::Method};
use error::Error;
use tower_http::{cors::{Any, CorsLayer}, services::ServeDir};

mod api;
mod error;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cors = cors();

    let static_dir = ServeDir::new("static");
    let app = Router::new()
        .nest("/api", api::routes())
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