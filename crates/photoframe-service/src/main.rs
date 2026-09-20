mod admin;
mod config;
mod db;
mod frame;

use std::{
    fs::{self, OpenOptions},
    net::SocketAddr,
    path::Path,
};

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
    ensure_runtime_paths(&config).context("failed runtime storage path validation")?;
    db::initialize(&config.database_path).context("failed to initialize sqlite database")?;
    let state = AppState {
        config: config.clone(),
    };

    let addr = SocketAddr::new(config.bind_address, config.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    let app = create_app(state);

    info!("photoframe service started on http://{addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server exited with error")?;

    info!("photoframe service stopped");

    Ok(())
}

fn ensure_runtime_paths(config: &AppConfig) -> Result<()> {
    ensure_directory_exists_and_writable(&config.image_dir, "image_dir")?;

    let db_parent = match config.database_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    ensure_directory_exists_and_writable(db_parent, "database_path parent directory")?;

    if config.database_path.is_dir() {
        anyhow::bail!(
            "database_path '{}' points to a directory, expected a file path",
            config.database_path.display()
        );
    }

    Ok(())
}

fn ensure_directory_exists_and_writable(path: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create {name} {}", path.display()))?;
    if !path.is_dir() {
        anyhow::bail!("{name} '{}' is not a directory", path.display());
    }

    let probe = path.join(".photoframe-write-probe");
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&probe)
        .with_context(|| format!("{name} '{}' is not writable", path.display()))?;
    fs::remove_file(&probe)
        .with_context(|| format!("failed to remove probe file '{}'", probe.display()))?;

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

fn create_app(state: AppState) -> Router {
    Router::new()
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
        .with_state(state)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    fn setup_test_app() -> (Router, std::path::PathBuf, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("photoframe-integ-{nanos}"));
        let img_dir = temp_dir.join("images");
        let db_path = temp_dir.join("photoframe.sqlite");

        std::fs::create_dir_all(&img_dir).unwrap();
        db::initialize(&db_path).unwrap();

        let config = AppConfig {
            image_dir: img_dir.clone(),
            database_path: db_path.clone(),
            ..Default::default()
        };

        let state = AppState { config };
        let app = create_app(state);
        (app, img_dir, db_path)
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let (app, img_dir, db_path) = setup_test_app();

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"ok");

        let _ = std::fs::remove_dir_all(img_dir.parent().unwrap());
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn test_frame_state_endpoint() {
        let (app, img_dir, db_path) = setup_test_app();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/frame/api/state")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("slideshow_interval_seconds"));
        assert!(body_str.contains("night_mode_active"));
        assert!(body_str.contains("images"));

        let _ = std::fs::remove_dir_all(img_dir.parent().unwrap());
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn test_admin_settings_lifecycle() {
        let (app, img_dir, db_path) = setup_test_app();

        // 1. GET settings
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/api/settings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // 2. POST update settings
        let payload = r#"{"slideshow_interval_seconds":45,"night_mode_start":"21:30","night_mode_end":"07:00"}"#;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/api/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // 3. GET settings again and verify updated values
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/api/settings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains(r#""slideshow_interval_seconds":45"#));
        assert!(body_str.contains(r#""night_mode_start":"21:30""#));
        assert!(body_str.contains(r#""night_mode_end":"07:00""#));

        let _ = std::fs::remove_dir_all(img_dir.parent().unwrap());
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn test_admin_and_frame_pages() {
        let (app, img_dir, db_path) = setup_test_app();

        let response = app
            .clone()
            .oneshot(Request::builder().uri("/admin").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("text/html")
        );
        let admin_body = response.into_body().collect().await.unwrap().to_bytes();
        let admin_html = String::from_utf8(admin_body.to_vec()).unwrap();
        assert!(admin_html.contains("repeat(5, 1fr)"));
        assert!(!admin_html.contains("image-meta"));

        let response = app
            .oneshot(Request::builder().uri("/frame").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("text/html")
        );

        let _ = std::fs::remove_dir_all(img_dir.parent().unwrap());
        let _ = std::fs::remove_file(db_path);
    }
}
