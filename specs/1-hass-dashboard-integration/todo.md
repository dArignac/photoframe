# Task List: Home Assistant Dashboard Screenshot Display

## Phase 1: Configuration & Database Foundation

- [x] **Task 1: Core Configuration (`config.rs`)**
  - Description: Extend `AppConfig` and `FileConfig` with `hass_dashboard_enabled` (default `false`), `hass_dashboard_url` (default `"http://localhost:8080"`), `hass_dashboard_display_seconds` (default `30`), and `hass_dashboard_refresh_interval_seconds` (default `60`). Implement environment variable resolution (`PHOTOFRAME_HASS_*`) and validation.
  - Acceptance:
    - Default configuration validates successfully.
    - Invalid durations/intervals or invalid URLs fail validation.
    - Environment variables and YAML overrides are respected.
  - Verify: `cargo test config::tests`
  - Files: `crates/photoframe-service/src/config.rs`
  - Estimated scope: S (1 file)

- [x] **Task 2: Database Persistence (`db.rs`)**
  - Description: Extend `AdminSettings` with the four new HASS fields. Update `load_admin_settings` and `save_admin_settings` to read/write them from/to the `settings` SQLite table.
  - Acceptance:
    - `load_admin_settings` returns defaults when keys are not in DB.
    - `save_admin_settings` persists new keys and reloads accurately.
  - Verify: `cargo test db::tests`
  - Files: `crates/photoframe-service/src/db.rs`
  - Estimated scope: S (1 file)

## Checkpoint 1: Foundation
- [x] Tests pass: `cargo test config::tests db::tests`
- [x] Code builds without warnings: `cargo check`

---

## Phase 2: Admin API & UI

- [x] **Task 3: Admin API Endpoints & Payload (`admin.rs`)**
  - Description: Update `AdminSettingsPayload` to serialize and deserialize the four HASS fields. Update `validate_settings_payload` with HASS validation rules. Update integration tests.
  - Acceptance:
    - `GET /admin/api/settings` includes all four HASS fields.
    - `POST /admin/api/settings` validates and updates settings.
    - Invalid payloads return `400 Bad Request`.
  - Verify: `cargo test tests::test_admin_settings_lifecycle`
  - Files: `crates/photoframe-service/src/admin.rs`
  - Estimated scope: S (1-2 files)

- [x] **Task 4: Admin UI Dedicated Section (`ADMIN_HTML` in `admin.rs`)**
  - Description: Add a dedicated `<section>` titled "Home Assistant Dashboard" in `ADMIN_HTML` with checkbox for enabled, inputs for URL, display duration, and refresh interval. Wire up loading and saving in JavaScript.
  - Acceptance:
    - Inputs populate from `/admin/api/settings`.
    - Form submission saves values to `/admin/api/settings` and displays feedback status.
  - Verify: `cargo test tests::test_admin_and_frame_pages`
  - Files: `crates/photoframe-service/src/admin.rs`
  - Estimated scope: S (1 file)

## Checkpoint 2: Admin Verification
- [x] Tests pass: `cargo test`
- [x] Admin HTML contains dedicated section and form inputs.

---

## Phase 3: Frame State API & Display View

- [x] **Task 5: Frame State Payload (`frame.rs`)**
  - Description: Update `FrameStatePayload` and `frame_state()` handler to include `hass_dashboard_enabled`, `hass_dashboard_url`, `hass_dashboard_display_seconds`, and `hass_dashboard_refresh_interval_seconds`.
  - Acceptance:
    - `GET /frame/api/state` returns current HASS settings.
  - Verify: `cargo test tests::test_frame_state_endpoint`
  - Files: `crates/photoframe-service/src/frame.rs`
  - Estimated scope: S (1 file)

- [x] **Task 6: Frame View Interaction & Periodic Refresh (`FRAME_HTML` in `frame.rs`)**
  - Description: Add `#hass-dashboard-image` DOM element with fullscreen fixed styling. Implement screen tap/click handling (show overlay, toggle off on second tap, resume slideshow on duration timeout). Implement background periodic fetch at configured interval, paused during night mode.
  - Acceptance:
    - Tap displays dashboard overlay when enabled.
    - Second tap or timeout dismisses overlay and resumes slideshow.
    - Periodic fetch runs only on configured interval.
    - Fetch paused during night mode.
  - Verify: `cargo test tests::test_admin_and_frame_pages`
  - Files: `crates/photoframe-service/src/frame.rs`
  - Estimated scope: M (1-2 files)

---

## Phase 4: Polish & Quality Gates

- [x] **Task 7: Documentation & Example Configuration**
  - Description: Update `config.example.yaml` and `README.md` to document the new HASS dashboard configuration options and integration guide.
  - Acceptance:
    - `config.example.yaml` includes commented HASS settings.
    - `README.md` documents all new settings and operational behavior.
  - Verify: Manual check of diff.
  - Files: `config.example.yaml`, `README.md`
  - Estimated scope: S (2 files)

- [x] **Task 8: Final Quality Gate Check**
  - Description: Run full test suite, format check, and clippy analysis.
  - Acceptance:
    - All tests pass (`cargo test`).
    - Zero clippy warnings (`cargo clippy --all-targets -- -D warnings`).
    - Format is clean (`cargo fmt --check`).
  - Verify: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
  - Files: All touched files
  - Estimated scope: XS
