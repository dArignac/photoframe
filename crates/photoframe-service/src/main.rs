mod admin;
mod config;
mod db;
mod frame;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};
use config::AppConfig;
use tokio::net::TcpListener;
use tracing::{info, warn};

#[derive(Clone)]
pub(crate) struct AppState {
    config: AppConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let config = AppConfig::load().context("failed to load configuration")?;
    db::initialize(&config.database_path).context("failed to initialize sqlite database")?;
    let state = AppState {
        config: config.clone(),
    };

    let addr = SocketAddr::new(config.bind_address, config.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/admin", get(admin::admin_page))
        .route("/images/{file_name}", get(admin::image_thumbnail))
        .route("/admin/images/{file_name}", get(admin::image_thumbnail))
        .route("/admin/api/images", get(admin::list_images))
        .route("/admin/api/images/{image_id}", delete(admin::delete_image))
        .route("/admin/api/upload", post(admin::upload_image))
        .route("/admin/api/reorder", post(admin::reorder_images))
        .route(
            "/admin/api/settings",
            get(admin::get_settings).post(admin::update_settings),
        )
        .route("/frame", get(frame::frame_page))
        .route("/frame/api/state", get(frame::frame_state))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 100))
        .with_state(state);

    info!("photoframe service started on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server exited with error")?;

    info!("photoframe service stopped");

    Ok(())
}

fn init_logging() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}

async fn health() -> &'static str {
    "ok"
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate_signal = signal(SignalKind::terminate())
            .expect("registering SIGTERM handler should succeed");

        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                if let Err(err) = signal {
                    warn!("failed waiting for Ctrl+C signal: {err}");
                }
            }
            _ = terminate_signal.recv() => {
                info!("received SIGTERM");
            }
        }
    }

    #[cfg(not(unix))]
    {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!("failed waiting for Ctrl+C signal: {err}");
        }
    }
}
