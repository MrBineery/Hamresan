mod routes;

use anyhow::Result;
use axum::Router;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

use crate::storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub url: String,
    pub pin: Option<String>,
    pub token: String,
    pub clipboard: Arc<RwLock<Option<String>>>,
}

impl AppState {
    pub fn new(folder: PathBuf, url: String, pin: Option<String>) -> Self {
        let token = pin.as_deref().map(token_from_pin).unwrap_or_default();
        Self {
            storage: Storage::new(folder),
            url,
            pin,
            token,
            clipboard: Arc::new(RwLock::new(None)),
        }
    }
}

pub fn token_from_pin(pin: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"hamresan-session-v1:");
    h.update(pin.as_bytes());
    hex::encode(h.finalize())
}

pub async fn run(port: u16, state: AppState) -> Result<()> {
    let app: Router = routes::router(state).layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    println!("\nShutting down...");
}