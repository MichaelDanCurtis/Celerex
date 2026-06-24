// packages/backend/api/src/main.rs
use celerex_api::{build_app, db::init_pool};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/celerex".into());
    let pool = init_pool(&url).await?;
    let app = build_app(pool);
    let port = env::var("PORT").unwrap_or_else(|_| "3001".into());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;
    println!("celerex-api listening on http://0.0.0.0:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}
