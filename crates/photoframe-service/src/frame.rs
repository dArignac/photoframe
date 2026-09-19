use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::State,
    response::{Html, IntoResponse},
};
use chrono::{Local, NaiveTime};
use serde::Serialize;

use crate::{
    AppState,
    admin::ApiError,
    db::{self, AdminSettings},
};

#[derive(Debug, Serialize)]
struct FrameImage {
    id: i64,
    file_name: String,
    url: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct FrameStatePayload {
    slideshow_interval_seconds: u64,
    frame_poll_interval_seconds: u64,
    display_fit_mode: &'static str,
    night_mode_active: bool,
    images: Vec<FrameImage>,
}

pub(crate) async fn frame_page() -> Html<&'static str> {
    Html(FRAME_HTML)
}

pub(crate) async fn frame_state(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let settings_defaults = AdminSettings {
        slideshow_interval_seconds: state.config.slideshow_interval_seconds,
        night_mode_start: state.config.night_mode_start.clone(),
        night_mode_end: state.config.night_mode_end.clone(),
    };
    let settings = db::load_admin_settings(&state.config.database_path, &settings_defaults)
        .map_err(ApiError::internal)?;
    let images = db::list_images(&state.config.database_path)
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|image| FrameImage {
            id: image.id,
            url: format!("/images/{}", image.file_name),
            file_name: image.file_name,
        })
        .collect::<Vec<_>>();

    let night_mode_active = is_night_mode_active(&settings).map_err(ApiError::internal)?;
    Ok(Json(FrameStatePayload {
        slideshow_interval_seconds: settings.slideshow_interval_seconds,
        frame_poll_interval_seconds: state.config.frame_poll_interval_seconds,
        display_fit_mode: state.config.display_fit_mode.as_str(),
        night_mode_active,
        images,
    }))
}

fn is_night_mode_active(settings: &AdminSettings) -> Result<bool> {
    let start = parse_hh_mm_time("night_mode_start", &settings.night_mode_start)?;
    let end = parse_hh_mm_time("night_mode_end", &settings.night_mode_end)?;
    let now = Local::now().time();

    if start < end {
        Ok(now >= start && now < end)
    } else if start > end {
        Ok(now >= start || now < end)
    } else {
        Ok(false)
    }
}

fn parse_hh_mm_time(name: &str, value: &str) -> Result<NaiveTime> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        bail!("{name} must be in HH:MM format");
    }

    let hour: u32 = parts[0]
        .parse()
        .with_context(|| format!("{name} has invalid hour component"))?;
    let minute: u32 = parts[1]
        .parse()
        .with_context(|| format!("{name} has invalid minute component"))?;

    NaiveTime::from_hms_opt(hour, minute, 0)
        .with_context(|| format!("{name} must be a valid 24-hour time"))
}

const FRAME_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>PhotoFrame</title>
  <style>
    html, body { margin: 0; width: 100%; height: 100%; background: #000; overflow: hidden; }
    #frame-root { width: 100vw; height: 100vh; display: flex; align-items: center; justify-content: center; background: #000; }
    #frame-image { width: 100vw; height: 100vh; object-fit: contain; display: none; background: #000; }
    #status-overlay {
      position: fixed;
      left: 16px;
      bottom: 16px;
      font-family: system-ui, sans-serif;
      font-size: 16px;
      color: #fff;
      background: rgba(0, 0, 0, 0.45);
      padding: 0.5rem 0.75rem;
      border-radius: 6px;
      display: none;
      pointer-events: none;
    }
  </style>
</head>
<body>
  <div id="frame-root">
    <img id="frame-image" alt="PhotoFrame image">
  </div>
  <div id="status-overlay"></div>

  <script>
    const frameImage = document.getElementById('frame-image');
    const statusOverlay = document.getElementById('status-overlay');
    let currentState = null;
    let rotationTimer = null;
    let pollTimer = null;
    let imageIndex = 0;

    function clearRotationTimer() {
      if (rotationTimer) {
        clearInterval(rotationTimer);
        rotationTimer = null;
      }
    }

    function clearPollTimer() {
      if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
      }
    }

    function setOverlay(text) {
      if (!text) {
        statusOverlay.style.display = 'none';
        statusOverlay.textContent = '';
        return;
      }
      statusOverlay.textContent = text;
      statusOverlay.style.display = 'block';
    }

    function renderCurrentImage() {
      if (!currentState) return;

      frameImage.style.objectFit = currentState.display_fit_mode;

      if (currentState.night_mode_active) {
        frameImage.style.display = 'none';
        setOverlay('Night mode active');
        return;
      }

      if (!currentState.images || currentState.images.length === 0) {
        frameImage.style.display = 'none';
        setOverlay('No images uploaded');
        return;
      }

      imageIndex = imageIndex % currentState.images.length;
      const image = currentState.images[imageIndex];
      frameImage.src = image.url;
      frameImage.style.display = 'block';
      setOverlay('');
    }

    function restartRotationTimer() {
      clearRotationTimer();
      if (!currentState || currentState.night_mode_active) return;
      if (!currentState.images || currentState.images.length <= 1) return;
      if (!currentState.slideshow_interval_seconds || currentState.slideshow_interval_seconds < 1) return;

      rotationTimer = setInterval(() => {
        imageIndex = (imageIndex + 1) % currentState.images.length;
        renderCurrentImage();
      }, currentState.slideshow_interval_seconds * 1000);
    }

    async function fetchState() {
      const response = await fetch('/frame/api/state', { cache: 'no-store' });
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}`);
      }
      return response.json();
    }

    function restartPollTimer() {
      clearPollTimer();
      const configuredSeconds = (currentState && currentState.frame_poll_interval_seconds)
        ? currentState.frame_poll_interval_seconds
        : 60;
      const seconds = Math.min(60, Math.max(1, configuredSeconds));
      pollTimer = setInterval(() => {
        refreshState().catch((error) => setOverlay(`Frame refresh error: ${error.message}`));
      }, seconds * 1000);
    }

    function keepCurrentImageIfPossible(previousState, nextState) {
      if (!previousState || !previousState.images || previousState.images.length === 0) {
        imageIndex = 0;
        return;
      }
      if (!nextState.images || nextState.images.length === 0) {
        imageIndex = 0;
        return;
      }

      const previousImage = previousState.images[imageIndex % previousState.images.length];
      const nextIndex = nextState.images.findIndex((image) => image.id === previousImage.id);
      imageIndex = nextIndex >= 0 ? nextIndex : 0;
    }

    async function refreshState() {
      const nextState = await fetchState();
      keepCurrentImageIfPossible(currentState, nextState);
      currentState = nextState;
      renderCurrentImage();
      restartRotationTimer();
      restartPollTimer();
    }

    refreshState().catch((error) => setOverlay(`Frame refresh error: ${error.message}`));
  </script>
</body>
</html>
"#;
