mod config;

use std::net::SocketAddr;

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
};
use config::AppConfig;
use tokio::net::TcpListener;
use tracing::{info, warn};

#[derive(Clone)]
struct AppState {
    config: AppConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let config = AppConfig::load().context("failed to load configuration")?;
    let state = AppState {
        config: config.clone(),
    };

    let addr = SocketAddr::new(config.bind_address, config.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    let app = Router::new()
        .route("/health", get(health))
        .route("/admin", get(admin))
        .route("/frame", get(frame))
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
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();
}

async fn health() -> &'static str {
    "ok"
}

async fn admin() -> impl IntoResponse {
    Html("<h1>PhotoFrame Admin</h1><p>Bootstrap complete.</p>")
}

async fn frame(State(state): State<AppState>) -> impl IntoResponse {
    let message = format!(
        "<h1>PhotoFrame Frame</h1><p>Interval: {}s</p>",
        state.config.slideshow_interval_seconds
    );
    Html(message)
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
