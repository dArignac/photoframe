# Implementation Plan: Home Assistant Dashboard Screenshot Display

## Overview
Implement on-demand display of a Home Assistant dashboard screenshot on `/frame` when a user taps or clicks the screen. The image is fetched from an external static URL served by `hass-dashboard-screenshot` (default: `http://localhost:8080`), kept fresh via periodic background fetching at a configurable interval (default: 60s), displayed for a configurable duration (default: 30s) or until tapped again (toggle off), and fully managed in a dedicated section in `/admin`.

## Architecture Decisions
1. **Client-side direct image loading**: The browser `/frame` client loads the screenshot directly via an `<img>` tag, avoiding backend proxying or HTTP client crate bloat in Rust.
2. **Background preloading**: An offscreen `Image` object fetches updates on the configured interval, ensuring 0ms display latency upon screen tap.
3. **Resource conservation**: Polling/refreshing is paused during active night mode.
4. **Key-value SQLite settings**: The new settings (`hass_dashboard_enabled`, `hass_dashboard_url`, `hass_dashboard_display_seconds`, `hass_dashboard_refresh_interval_seconds`) reuse the existing `settings` key-value table without destructive migrations.
5. **Atomic settings API**: `POST /admin/api/settings` accepts both slideshow and Home Assistant settings in a single unified payload.

---

## Implementation Order & Task Breakdown

### Phase 1: Configuration & Database Foundation
- **Task 1: Core Configuration (`config.rs`)**
  - Add `hass_dashboard_enabled`, `hass_dashboard_url`, `hass_dashboard_display_seconds`, and `hass_dashboard_refresh_interval_seconds` to `AppConfig` and `FileConfig`.
  - Add CLI, environment variable, and YAML file parsing.
  - Implement validation logic and unit tests.
- **Task 2: Database Persistence (`db.rs`)**
  - Extend `AdminSettings` struct with the four new fields.
  - Update `load_admin_settings` and `save_admin_settings` to handle the new keys.
  - Add unit tests for settings roundtrip and default fallbacks.

### Checkpoint: Foundation Verification
- Run `cargo test` to verify configuration validation and database persistence.

### Phase 2: Admin API & UI
- **Task 3: Admin API Endpoints & Payload (`admin.rs`)**
  - Extend `AdminSettingsPayload` with the four HASS fields.
  - Update payload validation in `validate_settings_payload`.
  - Add integration tests for `GET` and `POST /admin/api/settings`.
- **Task 4: Admin UI Dedicated Section (`ADMIN_HTML` in `admin.rs`)**
  - Add a dedicated `<section>` titled "Home Assistant Dashboard" with toggle, URL, duration, and refresh inputs.
  - Wire up form loading and saving to `/admin/api/settings` with status feedback.

### Checkpoint: Admin Verification
- Run `cargo test` to verify admin endpoint behavior and payload roundtrips.

### Phase 3: Frame State API & Display View
- **Task 5: Frame State Payload (`frame.rs`)**
  - Extend `FrameStatePayload` with the four HASS fields so the client receives them on state polls.
  - Add integration tests verifying `/frame/api/state` includes the new fields.
- **Task 6: Frame View Interaction & Periodic Refresh (`FRAME_HTML` in `frame.rs`)**
  - Add `<img id="hass-dashboard-image">` with fixed fullscreen overlay styles.
  - Implement tap/click event listener: toggle display on tap, auto-dismiss on timer expiration, pause/resume slideshow rotation.
  - Implement background periodic image refresh with cache-busting timestamp and night-mode pause.

### Checkpoint: Complete Verification & Documentation
- **Task 7: Documentation & Example Configuration**
  - Update `config.example.yaml` with the new fields.
  - Update `README.md` with configuration options and usage guide.
- **Task 8: End-to-End Quality Gate**
  - Run full test suite: `cargo test`.
  - Run linter and formatting: `cargo clippy --all-targets -- -D warnings && cargo fmt --check`.
