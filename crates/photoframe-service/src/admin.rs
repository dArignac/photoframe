use std::{
    io::ErrorKind,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{
    AppState,
    config::validate_hh_mm,
    db::{self, AdminSettings, StoredImage},
};

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
}

pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub(crate) fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub(crate) fn internal(err: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AdminSettingsPayload {
    slideshow_interval_seconds: u64,
    night_mode_start: String,
    night_mode_end: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ReorderPayload {
    ordered_ids: Vec<i64>,
}

pub(crate) async fn admin_page() -> Html<&'static str> {
    Html(ADMIN_HTML)
}

pub(crate) async fn image_thumbnail(
    AxumPath(file_name): AxumPath<String>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    if !is_safe_stored_file_name(&file_name) {
        return Err(ApiError::bad_request("invalid file name"));
    }

    let path = state.config.image_dir.join(&file_name);
    let bytes = match fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Err(ApiError {
                status: StatusCode::NOT_FOUND,
                message: "image not found".to_string(),
            });
        }
        Err(err) => {
            return Err(ApiError::internal(anyhow!(
                "failed to read image file {}: {err}",
                path.display()
            )));
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, mime_type_for_filename(&file_name)),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}

pub(crate) async fn list_images(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StoredImage>>> {
    let db_path = state.config.database_path.clone();
    let images = tokio::task::spawn_blocking(move || db::list_images(&db_path))
        .await
        .map_err(|err| ApiError::internal(anyhow!("spawn_blocking error: {err}")))?
        .map_err(ApiError::internal)?;
    Ok(Json(images))
}

pub(crate) async fn delete_image(
    AxumPath(image_id): AxumPath<i64>,
    State(state): State<AppState>,
) -> ApiResult<StatusCode> {
    let db_path = state.config.database_path.clone();
    let file_name_opt = tokio::task::spawn_blocking(move || db::delete_image(&db_path, image_id))
        .await
        .map_err(|err| ApiError::internal(anyhow!("spawn_blocking error: {err}")))?
        .map_err(ApiError::internal)?;

    let Some(file_name) = file_name_opt else {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: "image not found".to_string(),
        });
    };

    let file_path = state.config.image_dir.join(file_name);
    match fs::remove_file(&file_path).await {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => {
            return Err(ApiError::internal(anyhow!(
                "failed to remove image file {}: {err}",
                file_path.display()
            )));
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn upload_image(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<Vec<StoredImage>>> {
    fs::create_dir_all(&state.config.image_dir)
        .await
        .with_context(|| {
            format!(
                "failed to create image directory {}",
                state.config.image_dir.display()
            )
        })
        .map_err(ApiError::internal)?;

    let mut uploaded = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::internal(anyhow!("failed to read multipart field: {err}")))?
    {
        if field.name() != Some("image") {
            continue;
        }

        let original_name = field
            .file_name()
            .map(ToString::to_string)
            .unwrap_or_else(|| "upload.bin".to_string());

        if !is_allowed_image_extension(&original_name) {
            return Err(ApiError::bad_request(format!(
                "file '{original_name}' has an unsupported extension; allowed: jpg, jpeg, png, gif, webp, bmp"
            )));
        }

        let stored_name = make_stored_file_name(&original_name).map_err(ApiError::internal)?;

        let bytes = field
            .bytes()
            .await
            .map_err(|err| ApiError::internal(anyhow!("failed to read upload bytes: {err}")))?;
        if bytes.is_empty() {
            return Err(ApiError::bad_request("uploaded file is empty"));
        }

        let destination = state.config.image_dir.join(&stored_name);
        fs::write(&destination, bytes)
            .await
            .with_context(|| format!("failed to write uploaded file {}", destination.display()))
            .map_err(ApiError::internal)?;

        let db_path = state.config.database_path.clone();
        let stored_clone = stored_name.clone();
        let inserted = match tokio::task::spawn_blocking(move || {
            db::insert_image(&db_path, &stored_clone)
        })
        .await
        {
            Ok(Ok(img)) => img,
            Ok(Err(err)) => {
                let _ = fs::remove_file(&destination).await;
                return Err(ApiError::internal(err));
            }
            Err(err) => {
                let _ = fs::remove_file(&destination).await;
                return Err(ApiError::internal(anyhow!("spawn_blocking error: {err}")));
            }
        };
        uploaded.push(inserted);
    }

    if uploaded.is_empty() {
        return Err(ApiError::bad_request(
            "multipart field 'image' is required and must include at least one file",
        ));
    }

    Ok(Json(uploaded))
}

pub(crate) async fn reorder_images(
    State(state): State<AppState>,
    Json(payload): Json<ReorderPayload>,
) -> ApiResult<StatusCode> {
    let db_path = state.config.database_path.clone();
    tokio::task::spawn_blocking(move || db::reorder_images(&db_path, &payload.ordered_ids))
        .await
        .map_err(|err| ApiError::internal(anyhow!("spawn_blocking error: {err}")))?
        .map_err(|err| {
            let message = err.to_string();
            if message.contains("reorder payload must contain each image id exactly once") {
                ApiError::bad_request(message)
            } else {
                ApiError::internal(err)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn get_settings(
    State(state): State<AppState>,
) -> ApiResult<Json<AdminSettingsPayload>> {
    let defaults = AdminSettings {
        slideshow_interval_seconds: state.config.slideshow_interval_seconds,
        night_mode_start: state.config.night_mode_start.clone(),
        night_mode_end: state.config.night_mode_end.clone(),
    };
    let db_path = state.config.database_path.clone();
    let settings = tokio::task::spawn_blocking(move || db::load_admin_settings(&db_path, &defaults))
        .await
        .map_err(|err| ApiError::internal(anyhow!("spawn_blocking error: {err}")))?
        .map_err(ApiError::internal)?;
    Ok(Json(AdminSettingsPayload {
        slideshow_interval_seconds: settings.slideshow_interval_seconds,
        night_mode_start: settings.night_mode_start,
        night_mode_end: settings.night_mode_end,
    }))
}

pub(crate) async fn update_settings(
    State(state): State<AppState>,
    Json(payload): Json<AdminSettingsPayload>,
) -> ApiResult<StatusCode> {
    validate_settings_payload(&payload).map_err(|err| ApiError::bad_request(err.to_string()))?;

    let db_path = state.config.database_path.clone();
    let to_save = AdminSettings {
        slideshow_interval_seconds: payload.slideshow_interval_seconds,
        night_mode_start: payload.night_mode_start,
        night_mode_end: payload.night_mode_end,
    };
    tokio::task::spawn_blocking(move || db::save_admin_settings(&db_path, &to_save))
        .await
        .map_err(|err| ApiError::internal(anyhow!("spawn_blocking error: {err}")))?
        .map_err(ApiError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

fn validate_settings_payload(payload: &AdminSettingsPayload) -> Result<()> {
    if payload.slideshow_interval_seconds == 0 {
        bail!("slideshow_interval_seconds must be greater than 0");
    }
    validate_hh_mm("night_mode_start", &payload.night_mode_start)?;
    validate_hh_mm("night_mode_end", &payload.night_mode_end)?;
    Ok(())
}

fn make_stored_file_name(original_name: &str) -> Result<String> {
    let original = Path::new(original_name)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("upload.bin");

    let sanitized: String = original
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?
        .as_nanos();

    Ok(format!("{timestamp}-{sanitized}"))
}

pub(crate) fn is_allowed_image_extension(file_name: &str) -> bool {
    matches!(
        Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("jpg") | Some("jpeg") | Some("png") | Some("gif") | Some("webp") | Some("bmp")
    )
}

fn is_safe_stored_file_name(file_name: &str) -> bool {
    Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|base_name| base_name == file_name && !base_name.is_empty())
}

fn mime_type_for_filename(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

const ADMIN_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>PhotoFrame Admin</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 2rem auto; max-width: 860px; padding: 0 1rem; }
    h1, h2 { margin-bottom: 0.6rem; }
    section { border: 1px solid #ddd; border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }
    form { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
    ul { margin: 0.5rem 0 0; padding: 0; list-style: none; }
    li { padding: 0.45rem 0.55rem; border: 1px solid #ddd; border-radius: 6px; margin-bottom: 0.5rem; background: #fafafa; cursor: move; display: flex; gap: 0.75rem; align-items: center; }
    .row { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; }
    .status { min-height: 1.2rem; margin-top: 0.5rem; color: #114411; }
    .error { color: #8b0000; }
    input[type="number"] { width: 8rem; }
    .thumb { width: 100px; height: 70px; object-fit: cover; border-radius: 4px; border: 1px solid #ccc; background: #111; flex: none; }
    .image-meta { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .spacer { margin-left: auto; }
    .danger { border: 1px solid #a11; color: #a11; background: #fff; border-radius: 4px; padding: 0.25rem 0.45rem; cursor: pointer; }
  </style>
</head>
<body>
  <h1>PhotoFrame Admin</h1>

  <section>
    <h2>Upload image</h2>
    <form id="upload-form">
      <input id="image-input" name="image" type="file" accept="image/*" multiple required>
      <button type="submit">Upload</button>
    </form>
    <div id="upload-status" class="status"></div>
  </section>

  <section>
    <h2>Slideshow settings</h2>
    <form id="settings-form">
      <label class="row">Interval (seconds)
        <input id="slideshow_interval_seconds" type="number" min="1" required>
      </label>
      <label class="row">Night mode start (HH:MM)
        <input id="night_mode_start" type="text" placeholder="20:00" required>
      </label>
      <label class="row">Night mode end (HH:MM)
        <input id="night_mode_end" type="text" placeholder="06:00" required>
      </label>
      <button type="submit">Save settings</button>
    </form>
    <div id="settings-status" class="status"></div>
  </section>

  <section>
    <h2>Image order</h2>
    <p>Drag and drop to reorder, then save.</p>
    <ul id="image-list"></ul>
    <div class="row">
      <button id="save-order" type="button">Save order</button>
    </div>
    <div id="order-status" class="status"></div>
  </section>

  <script>
    const imageList = document.getElementById('image-list');
    const orderStatus = document.getElementById('order-status');
    const uploadStatus = document.getElementById('upload-status');
    const settingsStatus = document.getElementById('settings-status');
    let dragSource = null;

    function setStatus(el, message, isError = false) {
      el.textContent = message;
      el.classList.toggle('error', isError);
    }

    async function fetchJson(url, options = {}) {
      const response = await fetch(url, options);
      if (!response.ok) {
        let message = `HTTP ${response.status}`;
        try {
          const body = await response.json();
          if (body.error) message = body.error;
        } catch (_) {}
        throw new Error(message);
      }
      if (response.status === 204) return null;
      return response.json();
    }

    function makeImageItem(image) {
      const item = document.createElement('li');
      item.draggable = true;
      item.dataset.imageId = image.id;

      const thumb = document.createElement('img');
      thumb.className = 'thumb';
      thumb.src = `/admin/images/${encodeURIComponent(image.file_name)}`;
      thumb.alt = image.file_name;
      thumb.loading = 'lazy';

      const meta = document.createElement('div');
      meta.className = 'image-meta';
      meta.textContent = `${image.sort_index}. ${image.file_name} (${image.created_at})`;

      const spacer = document.createElement('div');
      spacer.className = 'spacer';

      const removeButton = document.createElement('button');
      removeButton.type = 'button';
      removeButton.className = 'danger';
      removeButton.textContent = 'Remove';
      removeButton.addEventListener('click', async (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (!confirm(`Remove image "${image.file_name}"?`)) return;
        setStatus(orderStatus, '');
        try {
          await fetchJson(`/admin/api/images/${image.id}`, { method: 'DELETE' });
          await refreshImages();
          setStatus(orderStatus, 'Image removed.');
        } catch (error) {
          setStatus(orderStatus, error.message, true);
        }
      });

      item.appendChild(thumb);
      item.appendChild(meta);
      item.appendChild(spacer);
      item.appendChild(removeButton);

      item.addEventListener('dragstart', () => {
        dragSource = item;
        item.style.opacity = '0.6';
      });
      item.addEventListener('dragend', () => {
        dragSource = null;
        item.style.opacity = '1';
      });
      item.addEventListener('dragover', (event) => event.preventDefault());
      item.addEventListener('drop', (event) => {
        event.preventDefault();
        if (!dragSource || dragSource === item) return;
        const listItems = [...imageList.querySelectorAll('li')];
        const sourceIndex = listItems.indexOf(dragSource);
        const targetIndex = listItems.indexOf(item);
        if (sourceIndex < targetIndex) {
          imageList.insertBefore(dragSource, item.nextSibling);
        } else {
          imageList.insertBefore(dragSource, item);
        }
      });

      return item;
    }

    async function refreshImages() {
      const images = await fetchJson('/admin/api/images');
      imageList.innerHTML = '';
      images.forEach((image) => imageList.appendChild(makeImageItem(image)));
    }

    async function refreshSettings() {
      const settings = await fetchJson('/admin/api/settings');
      document.getElementById('slideshow_interval_seconds').value = settings.slideshow_interval_seconds;
      document.getElementById('night_mode_start').value = settings.night_mode_start;
      document.getElementById('night_mode_end').value = settings.night_mode_end;
    }

    document.getElementById('upload-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      setStatus(uploadStatus, '');
      const input = document.getElementById('image-input');
      if (!input.files || input.files.length === 0) {
        setStatus(uploadStatus, 'Select a file to upload.', true);
        return;
      }

      const body = new FormData();
      for (const file of input.files) {
        body.append('image', file);
      }
      try {
        const uploaded = await fetchJson('/admin/api/upload', { method: 'POST', body });
        input.value = '';
        await refreshImages();
        setStatus(uploadStatus, `Upload complete (${uploaded.length} image${uploaded.length === 1 ? '' : 's'}).`);
      } catch (error) {
        setStatus(uploadStatus, error.message, true);
      }
    });

    document.getElementById('save-order').addEventListener('click', async () => {
      setStatus(orderStatus, '');
      const ordered_ids = [...imageList.querySelectorAll('li')].map((item) => Number(item.dataset.imageId));
      try {
        await fetchJson('/admin/api/reorder', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ ordered_ids })
        });
        await refreshImages();
        setStatus(orderStatus, 'Order saved.');
      } catch (error) {
        setStatus(orderStatus, error.message, true);
      }
    });

    document.getElementById('settings-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      setStatus(settingsStatus, '');
      const payload = {
        slideshow_interval_seconds: Number(document.getElementById('slideshow_interval_seconds').value),
        night_mode_start: document.getElementById('night_mode_start').value.trim(),
        night_mode_end: document.getElementById('night_mode_end').value.trim()
      };

      try {
        await fetchJson('/admin/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(payload)
        });
        setStatus(settingsStatus, 'Settings saved.');
      } catch (error) {
        setStatus(settingsStatus, error.message, true);
      }
    });

    Promise.all([refreshSettings(), refreshImages()]).catch((error) => {
      setStatus(orderStatus, error.message, true);
    });
  </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_image_extension() {
        assert!(is_allowed_image_extension("photo.jpg"));
        assert!(is_allowed_image_extension("photo.JPEG"));
        assert!(is_allowed_image_extension("photo.png"));
        assert!(is_allowed_image_extension("photo.gif"));
        assert!(is_allowed_image_extension("photo.webp"));
        assert!(is_allowed_image_extension("photo.bmp"));

        assert!(!is_allowed_image_extension("vector.svg"));
        assert!(!is_allowed_image_extension("page.html"));
        assert!(!is_allowed_image_extension("binary.exe"));
        assert!(!is_allowed_image_extension("script.sh"));
        assert!(!is_allowed_image_extension("no_extension"));
    }

    #[test]
    fn test_is_safe_stored_file_name() {
        assert!(is_safe_stored_file_name("12345-photo.jpg"));
        assert!(!is_safe_stored_file_name("../etc/passwd"));
        assert!(!is_safe_stored_file_name("foo/bar.jpg"));
        assert!(!is_safe_stored_file_name(".."));
        assert!(!is_safe_stored_file_name("."));
        assert!(!is_safe_stored_file_name(""));
    }

    #[test]
    fn test_make_stored_file_name() {
        let stored = make_stored_file_name("My Photo (1).jpg").unwrap();
        assert!(stored.ends_with("-My_Photo__1_.jpg"));
        assert!(!stored.contains(' '));
        assert!(!stored.contains('('));
        assert!(!stored.contains(')'));
    }
}
