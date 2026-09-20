use anyhow::Result;
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
    let db_path = state.config.database_path.clone();
    let (settings, raw_images) = tokio::task::spawn_blocking(move || {
        let settings = db::load_admin_settings(&db_path, &settings_defaults)?;
        let images = db::list_images(&db_path)?;
        Ok::<_, anyhow::Error>((settings, images))
    })
    .await
    .map_err(|err| ApiError::internal(anyhow::anyhow!("spawn_blocking error: {err}")))?
    .map_err(ApiError::internal)?;

    let images = raw_images
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
    let start = crate::config::parse_hh_mm("night_mode_start", &settings.night_mode_start)?;
    let end = crate::config::parse_hh_mm("night_mode_end", &settings.night_mode_end)?;
    let now = Local::now().time();
    Ok(is_time_in_window(now, start, end))
}

pub(crate) fn is_time_in_window(now: NaiveTime, start: NaiveTime, end: NaiveTime) -> bool {
    if start < end {
        now >= start && now < end
    } else if start > end {
        now >= start || now < end
    } else {
        false
    }
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
        setOverlay('');
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
        : 15;
      const seconds = Math.max(1, configuredSeconds);
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
      const previousState = currentState;
      keepCurrentImageIfPossible(previousState, nextState);
      currentState = nextState;

      const nightChanged = !previousState || previousState.night_mode_active !== nextState.night_mode_active;
      const intervalChanged = !previousState || previousState.slideshow_interval_seconds !== nextState.slideshow_interval_seconds;
      const imagesChanged = !previousState || JSON.stringify(previousState.images) !== JSON.stringify(nextState.images);
      const pollChanged = !previousState || previousState.frame_poll_interval_seconds !== nextState.frame_poll_interval_seconds;

      if (nightChanged || imagesChanged || !previousState) {
        renderCurrentImage();
      }

      if (nightChanged || intervalChanged || imagesChanged || !rotationTimer) {
        restartRotationTimer();
      }

      if (pollChanged || !pollTimer) {
        restartPollTimer();
      }
    }

    refreshState().catch((error) => setOverlay(`Frame refresh error: ${error.message}`));
  </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_time_in_window_daytime() {
        let start = NaiveTime::from_hms_opt(9, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(17, 0, 0).unwrap();

        assert!(!is_time_in_window(NaiveTime::from_hms_opt(8, 59, 59).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(9, 0, 0).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(12, 0, 0).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(16, 59, 59).unwrap(), start, end));
        assert!(!is_time_in_window(NaiveTime::from_hms_opt(17, 0, 0).unwrap(), start, end));
        assert!(!is_time_in_window(NaiveTime::from_hms_opt(22, 0, 0).unwrap(), start, end));
    }

    #[test]
    fn test_is_time_in_window_cross_midnight() {
        let start = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(6, 0, 0).unwrap();

        assert!(is_time_in_window(NaiveTime::from_hms_opt(20, 0, 0).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(23, 30, 0).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(0, 0, 0).unwrap(), start, end));
        assert!(is_time_in_window(NaiveTime::from_hms_opt(5, 59, 59).unwrap(), start, end));
        assert!(!is_time_in_window(NaiveTime::from_hms_opt(6, 0, 0).unwrap(), start, end));
        assert!(!is_time_in_window(NaiveTime::from_hms_opt(12, 0, 0).unwrap(), start, end));
        assert!(!is_time_in_window(NaiveTime::from_hms_opt(19, 59, 59).unwrap(), start, end));
    }

    #[test]
    fn test_is_time_in_window_equal_start_end() {
        let time = NaiveTime::from_hms_opt(8, 0, 0).unwrap();
        assert!(!is_time_in_window(time, time, time));
    }
}
