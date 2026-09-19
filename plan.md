# PhotoFrame implementation plan

## Problem and current state
- **Current state:** repository has no application code yet (greenfield start).
- **Goal:** build a browser-based photo frame app for Raspberry Pi with:
  - fullscreen slideshow with configurable interval
  - configurable night mode (black screen) including cross-midnight windows (example: 20:00-06:00)
  - image upload, reorder, and settings management UI
  - separate display entry point and management entry point
  - local network deployment (LAN reachable), no authentication
  - single-file database (SQLite)
  - configurable image folder and database folder
  - single binary packaged as a system package and auto-started on boot
  - high performance and low resource usage on older Raspberry Pi hardware

## Proposed architecture
- **Single process web server** serving:
  - `/frame` (display UI)
  - `/admin` (management UI)
  - small JSON/HTTP API for uploads, ordering, and settings
- **Storage split:**
  - metadata in SQLite file
  - image files on disk in configurable image directory
- **Night mode behavior:**
  - server calculates whether current local time is in night window
  - frame UI polls lightweight endpoint for state (day/night + current config)
  - if night is active, UI renders full black screen and suppresses image rotation
- **Ordering model:**
  - explicit `sort_index` per image row in DB
  - drag-and-drop reorder in admin UI updates `sort_index`
- **Runtime config:**
  - config file (default `/etc/photoframe/config.yaml`) + env/flag overrides
  - includes: bind address/port, image dir, DB dir/file, slideshow interval default, night-mode window, polling intervals, and display fit mode

## Suggested technologies (fit for constraints)
| Area | Recommendation | Why it fits |
|---|---|---|
| Language/runtime | Rust (stable) | Very low runtime overhead and strong safety for long-running service on constrained hardware |
| HTTP server | `axum` + `tokio` | Minimal, production-ready async stack with good performance/resource control |
| DB | SQLite single file | Matches requirement; simple local persistence |
| SQLite driver | `rusqlite` (with bundled SQLite) | Reliable SQLite integration with controlled single-binary packaging flow |
| Templates/UI | Server-rendered HTML templates + vanilla JS | Minimal RAM/CPU compared to SPA toolchains |
| Reordering UX | Native HTML Drag and Drop API | No dependency needed |
| Packaging | Debian package (`.deb`) with systemd unit | Natural fit for Raspberry Pi OS (Debian-based), supports autostart on boot |
| Config | YAML/TOML file + env/flags | Admin-friendly, operationally simple |

## Initial component design
1. **Core backend**
   - Rust HTTP server startup/shutdown
   - config loading/validation
   - static file serving for uploaded images
2. **Persistence**
   - SQLite schema:
     - `settings(key TEXT PRIMARY KEY, value TEXT NOT NULL)`
     - `images(id INTEGER PRIMARY KEY, file_name TEXT NOT NULL, sort_index INTEGER NOT NULL, created_at TEXT NOT NULL)`
   - migration bootstrap on startup
3. **Admin entry point (`/admin`)**
   - upload images
   - list images
   - reorder images
   - edit slideshow interval and night window
4. **Frame entry point (`/frame`)**
   - full-screen image rendering
   - timed image rotation
   - black-screen rendering during night mode
5. **Packaging/runtime**
   - build ARM binary (e.g., `armv7-unknown-linux-gnueabihf` and/or `aarch64-unknown-linux-gnu`)
   - `.deb` with post-install enabling/starting systemd service
   - runtime directories configurable and validated on startup

## Execution todos
1. **bootstrapping-rust-service**
   - Create Rust workspace/crate, app entrypoint, config loader, and graceful server lifecycle.
2. **defining-sqlite-schema**
   - Add DB initialization and migrations for `settings` and `images`.
3. **implementing-admin-http-flows**
   - Implement upload/list/reorder/settings endpoints and admin page.
4. **implementing-frame-rendering-flow**
   - Implement frame page, slide rotation config endpoint, and night-mode checks.
5. **wiring-file-storage-paths**
   - Ensure configurable image directory and DB directory with startup validation.
6. **creating-debian-packaging-and-service**
   - Add `.deb` packaging config and systemd unit for startup on boot.
7. **validating-resource-efficiency**
   - Run lightweight performance checks on Raspberry Pi profile; tune polling/caching defaults.

## Notes and tradeoffs
- **No image manipulation:** serve originals directly; frame CSS controls presentation (`object-fit`).
- **No auth:** acceptable only for trusted LAN; keep explicit warning in docs and service defaults.
- **LAN access:** default bind to `0.0.0.0` (configurable), with recommendation to isolate the network segment.
- **Night mode window crossing midnight:** treat `start > end` as cross-day range (e.g. 20:00-06:00).
- **Timezone source:** use Raspberry Pi system timezone for night-mode checks.
