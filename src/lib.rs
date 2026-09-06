#![forbid(unsafe_code)]

mod crypto;
mod db;
mod engine;
mod resources;
pub mod templates;
mod web;

use std::net::SocketAddr;
use std::path::PathBuf;

use auth_mini_axum::{AuthMiniLayer, JwksCachePolicy};
use db::Database;
use engine::TraderRuntime;
use thiserror::Error;

pub use templates::{AccountCredential, exchanges, runtime};
pub use web::router;

#[derive(Clone, Debug)]
pub struct App {
    database: Database,
    runtime: TraderRuntime,
}

#[derive(Debug, Error)]
pub enum HttpServerError {
    #[error("failed to prepare HIT data directory")]
    DataDirectory(#[source] std::io::Error),
    #[error("failed to initialize HIT database")]
    Database(#[from] db::DatabaseError),
    #[error("failed to initialize Auth Mini verification")]
    Auth(#[from] auth_mini_axum::AuthMiniError),
    #[error("HTTP server I/O error")]
    Io(#[from] std::io::Error),
}

impl App {
    /// Creates HIT from its deliberately small, user-home based runtime state.
    ///
    /// # Errors
    ///
    /// Returns an error when the home directory, encryption key or `SQLite` state cannot be used.
    pub fn from_home() -> Result<Self, HttpServerError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let state_directory = home.join(".hit");
        std::fs::create_dir_all(&state_directory).map_err(HttpServerError::DataDirectory)?;
        let database = Database::open(state_directory)?;
        let runtime = TraderRuntime::new(database.clone());
        Ok(Self { database, runtime })
    }

    /// Serves the embedded web application and its API.
    ///
    /// # Errors
    ///
    /// Returns an error when the socket cannot bind or the server exits unexpectedly.
    pub async fn serve(self, address: SocketAddr) -> Result<(), HttpServerError> {
        self.runtime.reconcile().await;
        let auth = AuthMiniLayer::from_issuer_background(
            "https://auth.ntnl.io",
            "hit.ntnl.io",
            JwksCachePolicy::default(),
        )?;
        let app = web::router(self.database, self.runtime, auth);
        let listener = tokio::net::TcpListener::bind(address).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
