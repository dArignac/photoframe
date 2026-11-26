# Spec: Home Assistant Dashboard Screenshot Display

## Objective

Provide on-demand display of a Home Assistant dashboard screenshot on the digital photo frame (`/frame`) when a user taps on a touchscreen or clicks with a mouse. The dashboard screenshot is rendered from an external static image URL provided by [`dArignac/hass-dashboard-screenshot`](https://github.com/dArignac/hass-dashboard-screenshot) (default: `http://localhost:8080`).

Key operational characteristics:
- **On-Demand Overlay:** Tapping or clicking anywhere on the `/frame` screen immediately overlays the Home Assistant dashboard image over the photo slideshow.
- **Configurable Display Duration:** The dashboard remains visible for a configurable amount of time (default: 30 seconds), after which the slideshow resumes seamlessly.
- **Configurable Periodic Background Fetch:** To minimize resource usage (network bandwidth, CPU, and memory) on low-power devices like the Raspberry Pi, the frame client only fetches updated screenshot images at a configurable background interval (default: 60 seconds).
- **Admin Configuration:** All settings (enabled toggle, dashboard image URL, display duration, and refresh interval) must be viewable and editable via the `/admin` web interface and persisted in SQLite.
- **Zero External Runtime Dependencies:** Implemented using existing Rust/Axum backend and embedded vanilla HTML5/JavaScript frontend without introducing additional runtime crates or client-side npm libraries.

---

## Tech Stack

- **Backend Language & Runtime:** Rust (2024 Edition), Tokio 1.48 (multi-threaded async runtime)
- **Web Framework:** Axum 0.8 (HTTP routing, JSON serialization/deserialization, multipart upload)
- **Storage:** Embedded SQLite via `rusqlite` 0.37 (`bundled` feature, WAL mode enabled)
- **Configuration:** Serde 1.0, `serde_yaml` 0.9, `clap` 4.5
- **Display & Admin Frontends:** Zero-build vanilla HTML5, CSS3, and ES6+ JavaScript embedded as string literals in Rust (`ADMIN_HTML` in `admin.rs`, `FRAME_HTML` in `frame.rs`)
- **External Integration Target:** [`dArignac/hass-dashboard-screenshot`](https://github.com/dArignac/hass-dashboard-screenshot) serving static screenshot images (e.g., `http://localhost:8080` or `http://localhost:8080/screenshot.png`)

---

## Commands

```bash
# Build (debug)
cargo build

# Build (release binary)
cargo build --release

# Run all unit and integration tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Lint and static analysis
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Format code
cargo fmt

# Run locally in development mode with local config
cargo run -- --config config.yaml
```

---

## Project Structure

```text
photoframe/
├── Cargo.toml                                 # Cargo workspace definition
├── Cargo.lock                                 # Pinned dependency tree
├── config.example.yaml                        # Configuration template (includes new HASS settings)
├── config.yaml                                # Local development config (gitignored)
├── photoframe.sqlite                          # SQLite database
├── specs/
│   └── SPEC-home-assistant-dashboard.md       # This specification
└── crates/
    └── photoframe-service/
        ├── Cargo.toml                         # Service dependencies
        └── src/
            ├── main.rs                        # Axum router setup, startup logging, health check
            ├── config.rs                      # AppConfig struct, CLI/Env/YAML parsing & validation
            ├── db.rs                          # SQLite schema, settings load/save
            ├── admin.rs                       # Admin HTML UI, settings API endpoints, validations
            └── frame.rs                       # Fullscreen frame HTML, state API endpoint, client-side logic
```

---

## Configuration & Schema Specification

### 1. Configuration Fields

| Setting Name | DB / JSON Key | Type | Default | Validation Rules | Description |
|---|---|---|---|---|---|
| Dashboard Enabled | `hass_dashboard_enabled` | `bool` | `false` | Boolean (`true`/`false`) | Master toggle for the dashboard overlay feature |
| Dashboard Image URL | `hass_dashboard_url` | `String` | `"http://localhost:8080"` | Valid HTTP/HTTPS URL when enabled; non-empty | Endpoint serving the static screenshot image |
| Display Duration | `hass_dashboard_display_seconds` | `u64` | `30` | Min: 1, Max: 86400 | Seconds to show the dashboard on tap before resuming slideshow |
| Image Refresh Interval | `hass_dashboard_refresh_interval_seconds` | `u64` | `60` | Min: 5, Max: 86400 | Interval in seconds between background image fetches |

### 2. Configuration Precedence

1. **SQLite Database (`settings` table)**: Values saved via `/admin` take highest runtime precedence.
2. **Environment Variables**:
   - `PHOTOFRAME_HASS_DASHBOARD_ENABLED` (`"true"` | `"false"`)
   - `PHOTOFRAME_HASS_DASHBOARD_URL`
   - `PHOTOFRAME_HASS_DASHBOARD_DISPLAY_SECONDS`
   - `PHOTOFRAME_HASS_DASHBOARD_REFRESH_INTERVAL_SECONDS`
3. **YAML Configuration File (`config.yaml`)**:
   ```yaml
   hass_dashboard_enabled: false
   hass_dashboard_url: "http://localhost:8080"
   hass_dashboard_display_seconds: 30
   hass_dashboard_refresh_interval_seconds: 60
   ```
4. **Compiled-in Application Defaults** (`AppConfig::default()`).

### 3. Database Schema

The existing SQLite database stores configuration in the key-value `settings` table:
```sql
CREATE TABLE IF NOT EXISTS settings(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

The new keys stored in `settings`:
- `hass_dashboard_enabled` -> `"true"` or `"false"`
- `hass_dashboard_url` -> e.g. `"http://localhost:8080"`
- `hass_dashboard_display_seconds` -> e.g. `"30"`
- `hass_dashboard_refresh_interval_seconds` -> e.g. `"60"`

*No destructive migration or new tables required.* `db::load_admin_settings` selects these keys and falls back to `AppConfig` defaults if absent.

---

## API Contracts

### 1. Admin Settings: `GET /admin/api/settings`

**Response (`200 OK`):**
```json
{
  "slideshow_interval_seconds": 30,
  "night_mode_start": "20:00",
  "night_mode_end": "06:00",
  "hass_dashboard_enabled": true,
  "hass_dashboard_url": "http://localhost:8080",
  "hass_dashboard_display_seconds": 30,
  "hass_dashboard_refresh_interval_seconds": 60
}
```

### 2. Admin Settings: `POST /admin/api/settings`

**Request Body (`application/json`):**
```json
{
  "slideshow_interval_seconds": 45,
  "night_mode_start": "21:00",
  "night_mode_end": "07:00",
  "hass_dashboard_enabled": true,
  "hass_dashboard_url": "http://192.168.1.50:8080/screenshot.png",
  "hass_dashboard_display_seconds": 30,
  "hass_dashboard_refresh_interval_seconds": 60
}
```

**Responses:**
- `204 No Content` on successful save.
- `400 Bad Request` on validation error:
  ```json
  { "error": "hass_dashboard_display_seconds must be greater than 0" }
  ```
  ```json
  { "error": "hass_dashboard_url must be a valid http or https URL" }
  ```

### 3. Frame State: `GET /frame/api/state`

**Response (`200 OK`):**
```json
{
  "slideshow_interval_seconds": 30,
  "frame_poll_interval_seconds": 15,
  "display_fit_mode": "contain",
  "night_mode_active": false,
  "hass_dashboard_enabled": true,
  "hass_dashboard_url": "http://localhost:8080",
  "hass_dashboard_display_seconds": 30,
  "hass_dashboard_refresh_interval_seconds": 60,
  "images": [
    {
      "id": 1,
      "file_name": "1732620000000000000-photo.jpg",
      "url": "/images/1732620000000000000-photo.jpg"
    }
  ]
}
```

---

## Detailed Frontend & Interaction Mechanics

### 1. Frame Client (`/frame`)

1. **DOM Elements:**
   - Add a dedicated dashboard image element:
     ```html
     <div id="frame-root">
       <img id="frame-image" alt="PhotoFrame image">
       <img id="hass-dashboard-image" alt="Home Assistant Dashboard" style="display: none;">
     </div>
     ```
   - `#hass-dashboard-image` is styled with `position: fixed; inset: 0; width: 100vw; height: 100vh; object-fit: contain; z-index: 10; background: #000;`.

2. **Touch & Click Event Handling:**
   - Attach listener to `window` or `#frame-root` using `pointerup` (or combined `click` and touch handling) to prevent mobile touch delay and avoid duplicate firing.
   - Ignore interaction if `hass_dashboard_enabled` is `false`, or if `hass_dashboard_url` is blank.
   - If `night_mode_active` is `true`:
     - Tapping shows the dashboard for `hass_dashboard_display_seconds`, then returns to night blackout (or stays black if disabled during night mode — see Open Questions).

3. **Display Lifecycle & Slideshow Resumption:**
   - When triggered:
     - If dashboard is already visible: tapping again immediately dismisses the dashboard and resumes slideshow (toggle behavior) OR resets the display duration timer (see Open Questions).
     - `#hass-dashboard-image` display set to `'block'`.
     - Slideshow rotation interval is suspended while dashboard is displayed.
     - A display timer is started for `hass_dashboard_display_seconds * 1000`.
   - When display timer expires:
     - `#hass-dashboard-image` display set to `'none'`.
     - Slideshow timer restarts and current photo resumes displaying.

4. **Periodic Background Fetch & Resource Optimization:**
   - The browser client maintains a background refresh timer set to `hass_dashboard_refresh_interval_seconds * 1000`.
   - On each interval tick (and initially upon page load/state sync):
     - An offscreen `new Image()` is instantiated with the URL appended with a cache-busting timestamp query parameter (e.g., `url + (url.includes('?') ? '&' : '?') + '_t=' + Date.now()`).
     - Once `onload` fires, `#hass-dashboard-image.src` is updated to the newly loaded image URL.
     - If `onerror` fires (e.g. `hass-dashboard-screenshot` service temporarily returning 503 or network glitch), the error is logged or quietly ignored, preserving the previous valid image without interrupting the slideshow.
   - If the dashboard is currently visible on screen, updating the image in place keeps the dashboard view fresh.
   - During `night_mode_active`, background fetching is paused to conserve CPU/network on the Raspberry Pi and server.

### 2. Admin Client (`/admin`)

- Add a new section to `/admin`:
  ```html
  <section>
    <h2>Home Assistant Dashboard</h2>
    <form id="hass-settings-form">
      <label class="row">
        <input id="hass_dashboard_enabled" type="checkbox"> Enable dashboard on screen tap/click
      </label>
      <label class="row">Dashboard Screenshot URL
        <input id="hass_dashboard_url" type="url" placeholder="http://localhost:8080" required>
      </label>
      <label class="row">Display duration (seconds)
        <input id="hass_dashboard_display_seconds" type="number" min="1" max="86400" required>
      </label>
      <label class="row">Refresh interval (seconds)
        <input id="hass_dashboard_refresh_interval_seconds" type="number" min="5" max="86400" required>
      </label>
      <button type="submit">Save Dashboard Settings</button>
    </form>
    <div id="hass-settings-status" class="status"></div>
  </section>
  ```
- Either combined with existing settings form or as its own section matching existing admin aesthetic and UX.

---

## Code Style

- **Rust:** Standard `rustfmt` rules, 4-space indentation, strict error handling with `anyhow` and descriptive error contexts.
- **Embedded Frontend:** Clean ES6+, 2-space indentation, no external dependencies, semantic HTML, vanilla DOM API (`document.getElementById`, `fetch`).

Example Rust struct pattern:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminSettings {
    pub slideshow_interval_seconds: u64,
    pub night_mode_start: String,
    pub night_mode_end: String,
    pub hass_dashboard_enabled: bool,
    pub hass_dashboard_url: String,
    pub hass_dashboard_display_seconds: u64,
    pub hass_dashboard_refresh_interval_seconds: u64,
}
```

---

## Testing Strategy

1. **Unit Tests (`config.rs`):**
   - Validation tests for `hass_dashboard_display_seconds` (> 0).
   - Validation tests for `hass_dashboard_refresh_interval_seconds` (>= 5).
   - Validation of URL parsing (valid schemes: `http`, `https`).
   - Default configuration validation.

2. **Database Tests (`db.rs`):**
   - Test `load_admin_settings` with existing defaults.
   - Test `save_admin_settings` with new HASS fields and verify roundtrip retrieval.

3. **Integration Tests (`main.rs` / `admin.rs` / `frame.rs`):**
   - `GET /admin/api/settings` returns all HASS dashboard settings fields.
   - `POST /admin/api/settings` updates HASS dashboard settings and validates invalid inputs (e.g., negative/zero duration, malformed URL).
   - `GET /frame/api/state` includes HASS settings in state payload.
   - Admin and frame HTML contains required elements (`#hass-dashboard-image`, `#hass-settings-form`).

4. **Manual & Kiosk Verification:**
   - Kiosk browser test on Chromium: clicking/tapping displays the dashboard overlay.
   - Timer test: after 30 seconds (or configured duration), overlay disappears and slideshow resumes.
   - Network test: verify network activity occurs only once per configured interval (e.g. 60s).

---

## Boundaries

- **Always:**
  - Run `cargo test` and `cargo clippy --all-targets -- -D warnings` before committing.
  - Validate all inputs on the backend (URL validity, positive integers).
  - Handle image fetch errors gracefully so slideshow never crashes.
- **Ask First:**
  - Adding new third-party crate dependencies to `Cargo.toml`.
  - Adding server-side image proxying (which would add network I/O and HTTP client overhead to the Rust binary).
- **Never:**
  - Remove existing tests or weaken test assertions.
  - Break existing configuration format or defaults for users who do not use Home Assistant.
  - Introduce Node.js/npm or external runtime build steps for the frontend.

---

## Success Criteria

1. **SC-1 (Tap/Click Overlay):** When touching or clicking anywhere on the `/frame` page, the configured Home Assistant dashboard screenshot appears instantly on screen.
2. **SC-2 (Display Timer):** The dashboard remains on screen for the configured display duration (default: 30s), then automatically dismisses and the photo slideshow resumes without skipping or stuttering.
3. **SC-3 (Configurable Refresh Interval):** The `/frame` client only fetches an updated screenshot from the configured URL according to the configured refresh interval (default: 60s), minimizing resource allocation on the device.
4. **SC-4 (Admin Interface):** The `/admin` UI displays a dedicated form to configure:
   - Enable/disable toggle
   - Dashboard URL (default `http://localhost:8080`)
   - Display duration (default `30`)
   - Refresh interval (default `60`)
   Values are successfully saved and persisted across service restarts.
5. **SC-5 (Dynamic Live Update):** When settings are modified in `/admin`, the `/frame` client adopts the new configuration on its next state poll without requiring a page reload.
6. **SC-6 (Resilience & Error Handling):** If the Home Assistant dashboard service is unreachable or returns an HTTP error, the photoframe logs the error cleanly and the photo slideshow continues uninterrupted.
7. **SC-7 (Quality Gates):** All unit and integration tests pass cleanly with `cargo test`, and `cargo clippy` produces 0 warnings.

---

## Assumptions & Design Decisions

1. **Client-side Fetching:** The `/frame` web client fetches the static screenshot directly from `hass_dashboard_url` via an HTML `<img>` element. Because standard `<img>` tags are not subject to CORS cross-origin restrictions, the browser can load the image from port 8080 directly.
2. **Preloading for Instant Display:** The `/frame` client keeps a preloaded image in the background updated at the configured interval. When the user taps, the image displays with 0ms latency rather than triggering a network fetch after the tap.
3. **Night Mode Behavior:** If tapped during night mode hours, the dashboard appears for the display duration, and then the screen returns to black (night mode). Background refresh is paused during night mode to conserve resources.
4. **Second Tap Action (Option A - Toggle):** If the user taps while the dashboard is already showing, the dashboard closes immediately and resumes the photo slideshow.
5. **Default State:** The feature is disabled by default (`hass_dashboard_enabled: false`) to prevent unexpected blank overlay behaviors for users not using Home Assistant.
6. **Admin UI Section:** Home Assistant dashboard settings are housed in their own dedicated "Home Assistant Dashboard" section in the admin view.

---

## Open Questions (Resolved)

1. **Second Tap Action:** *Resolved: Option A.* Second tap dismisses the dashboard immediately and resumes the slideshow.
2. **Settings Section Layout:** *Resolved.* Placed in a dedicated "Home Assistant Dashboard" section on `/admin`.
3. **Default State:** *Resolved.* Default is disabled (`hass_dashboard_enabled: false`).
