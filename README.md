# PhotoFrame

A lightweight, self-contained digital photo frame web service designed for the Raspberry Pi and local home networks. Built for minimal resource usage, high reliability, and zero external runtime dependencies.

---

## Features

- **Resource Efficient & Self-Contained:** Written in Rust with Axum and embedded SQLite (`rusqlite` bundled). Single static binary with embedded HTML/JS/CSS. Minimal RAM (< 20 MB) and CPU footprint.
- **Dedicated Fullscreen Display (`/frame`):** Clean, distraction-free slideshow view with configurable interval, automatic image transitions, and choice of aspect-ratio fit modes (`contain` with black bars or `cover` borderless fill).
- **Automated Night Mode:** Automatically blacks out the frame during configured hours (e.g., `20:00` to `06:00`), seamlessly supporting schedules that cross midnight.
- **Admin Management Dashboard (`/admin`):** Simple web UI to upload photos (multi-file support for JPEG, PNG, WebP, GIF, BMP), remove photos, drag-and-drop to reorder the slideshow, and adjust settings on the fly.
- **Dynamic Live Updates:** The fullscreen display client polls state automatically; changes to images, order, intervals, or night mode are reflected without reloading or restarting the service.
- **Ready for Raspberry Pi OS:** Packaged as a standard Debian package (`.deb`) with systemd service configuration, automatic startup on boot, and security sandboxing.

---

## Table of Contents

- [User Guide](#user-guide)
  - [Installation on Raspberry Pi](#1-installation-on-raspberry-pi)
  - [Prerequisites & Kiosk Setup](#2-prerequisites--kiosk-setup)
  - [Configuration](#3-configuration)
  - [Managing the Service](#4-managing-the-service)
  - [Using the Web Interfaces](#5-using-the-web-interfaces)
- [Developer Guide](#developer-guide)
  - [Architecture & Tech Stack](#architecture--tech-stack)
  - [Repository & Code Structure](#repository--code-structure)
  - [Local Development Setup](#local-development-setup)
  - [Testing & Quality Checks](#testing--quality-checks)
  - [Building the Debian Package](#building-the-debian-package)
  - [Database Schema & Migrations](#database-schema--migrations)
  - [HTTP API Reference](#http-api-reference)

---

# User Guide

## 1. Installation on Raspberry Pi

Pre-built Debian packages are available on the repository's [GitHub Releases](https://github.com/dArignac/photoframe/releases) page for ARMv7 / 32-bit (`armhf`, compatible with Raspberry Pi 2/3/4/Zero 2 running Raspberry Pi OS).

### Step 1: Download the Package

On your Raspberry Pi, download the latest `.deb` release:

```bash
# Example for version 0.1.2 (replace with desired release version)
wget https://github.com/dArignac/photoframe/releases/download/v0.1.2/photoframe_0.1.2_armhf.deb
```

### Step 2: Install via APT

Install the package using `apt` (which automatically verifies and satisfies dependencies):

```bash
sudo apt install ./photoframe_0.1.2_armhf.deb
```

_(Alternatively, use `sudo dpkg -i photoframe_0.1.2_armhf.deb` followed by `sudo apt-get install -f` if dependencies need resolving)._

### What the Package Sets Up Automatically

- **Executable:** `/usr/bin/photoframe-service`
- **System User & Group:** Creates an isolated, unprivileged system account `photoframe:photoframe`
- **Configuration File:** `/etc/photoframe/config.yaml` (protected Debian `conffile`; upgrades will never overwrite your custom configuration)
- **Data Directories:** `/etc/photoframe/` and `/etc/photoframe/images/` with permissions assigned to the `photoframe` user
- **Systemd Service:** `/lib/systemd/system/photoframe.service` — automatically enabled and started on system boot

---

## 2. Prerequisites & Kiosk Setup

### Required System Packages

The core service requires only modern Debian-compatible runtime libraries:

- `libc6 (>= 2.31)` (included in Raspberry Pi OS Bullseye, Bookworm, and newer)
- `systemd`

### Fullscreen Kiosk Setup (Displaying `/frame` on Screen)

Since `photoframe-service` runs as a headless web server, you will need a web browser running on your Raspberry Pi screen pointing to `http://localhost:8181/frame`.

#### 1. Install Display & Browser Utilities

```bash
sudo apt update
sudo apt install -y chromium-browser unclutter xdotool
```

- `chromium-browser`: Displays the frame in fullscreen kiosk mode.
- `unclutter`: Automatically hides the mouse cursor when idle.
- `xdotool` / `x11-xserver-utils`: Manages display power and screen blanking.

#### 2. Configure Kiosk Autostart on Raspberry Pi OS Desktop (X11)

Create or edit the desktop autostart configuration (e.g. `~/.config/lxsession/LXDE-pi/autostart` or `/etc/xdg/lxsession/LXDE-pi/autostart`):

```bash
# Disable screen blanking and power-saving sleep
@xset s off
@xset -dpms
@xset s noblank

# Hide mouse cursor after 0.5s of inactivity
@unclutter -idle 0.5 -root

# Launch Chromium in fullscreen kiosk mode pointing to the local PhotoFrame endpoint
@chromium-browser --kiosk --noerrdialogs --disable-infobars --no-first-run --check-for-update-interval=31536000 http://localhost:8181/frame
```

> **Tip for Wayland (Raspberry Pi OS Bookworm with Wayfire / Labwc):**
> You can launch Chromium in kiosk mode via `wayfire.ini` autostart, or use a lightweight Wayland kiosk compositor like `cage`:
>
> ```bash
> cage -- chromium-browser --kiosk http://localhost:8181/frame
> ```

---

## 3. Configuration

The default configuration file is located at `/etc/photoframe/config.yaml`:

```yaml
# Network bind address (0.0.0.0 allows LAN access; use 127.0.0.1 for local only)
bind_address: 0.0.0.0

# TCP port to listen on
port: 8181

# Directory where uploaded photo files are saved
image_dir: /etc/photoframe/images

# Path to the SQLite database file storing image metadata and admin settings
database_path: /etc/photoframe/photoframe.sqlite

# Default display duration per photo (in seconds)
slideshow_interval_seconds: 30

# Night mode start time (24-hour HH:MM format)
night_mode_start: "20:00"

# Night mode end time (24-hour HH:MM format)
night_mode_end: "06:00"

# How frequently the frame client checks the server for updates (in seconds)
frame_poll_interval_seconds: 15

# Image scaling mode: "contain" (letterboxed, shows full image) or "cover" (crops to fill display)
display_fit_mode: contain
```

### Configuration Options Explained

| Key                           | Default                             | Description                                                                                                                                         |
| ----------------------------- | ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bind_address`                | `0.0.0.0`                           | IP address to bind. Set to `0.0.0.0` so you can access the admin dashboard from any device on your local Wi-Fi / LAN.                               |
| `port`                        | `8181`                              | TCP port for both the display and admin dashboard.                                                                                                  |
| `image_dir`                   | `/etc/photoframe/images`            | Storage directory for uploaded images. Must be writable by the `photoframe` user.                                                                   |
| `database_path`               | `/etc/photoframe/photoframe.sqlite` | SQLite database file location. Parent directory must be writable.                                                                                   |
| `slideshow_interval_seconds`  | `30`                                | Number of seconds each image is displayed before rotating to the next.                                                                              |
| `night_mode_start`            | `"20:00"`                           | 24-hour clock time `HH:MM` when screen blackout starts.                                                                                             |
| `night_mode_end`              | `"06:00"`                           | 24-hour clock time `HH:MM` when screen blackout ends. Supports midnight-crossing windows (e.g. `20:00` to `06:00`).                                 |
| `frame_poll_interval_seconds` | `15`                                | Polling frequency for the `/frame` browser client to check for new images, settings, or night mode state.                                           |
| `display_fit_mode`            | `contain`                           | How images fit the screen: `contain` preserves the original aspect ratio with black bars, while `cover` fills the screen, cropping edges if needed. |

### Configuration Precedence

Settings are resolved in the following priority order (highest to lowest):

1. **CLI Flags** (`--bind-address`, `--port`)
2. **Environment Variables** (`PHOTOFRAME_*`, e.g., `PHOTOFRAME_PORT`, `PHOTOFRAME_IMAGE_DIR`)
3. **YAML Configuration File** (`--config <path>`, defaults to `/etc/photoframe/config.yaml`)
4. **Compiled-in Application Defaults**

> **Note:** Slideshow interval and night mode times can also be modified live through the `/admin` web interface without editing files. Values saved via the admin interface take precedence over the config file defaults.

---

## 4. Managing the Service

`photoframe` runs as a systemd service under the name `photoframe.service`.

```bash
# Check service status and health
sudo systemctl status photoframe.service

# Restart the service (e.g., after modifying /etc/photoframe/config.yaml)
sudo systemctl restart photoframe.service

# Stop the service
sudo systemctl stop photoframe.service

# Start the service
sudo systemctl start photoframe.service

# Follow live service logs
journalctl -u photoframe.service -f
```

### Upgrades & Uninstallation

- **Upgrade:** Simply install the new `.deb` file using `sudo apt install ./photoframe_<new_version>_armhf.deb`. Your database, images, and configuration file in `/etc/photoframe/` are preserved.
- **Uninstall (keep data):**
  ```bash
  sudo apt remove photoframe
  ```
- **Purge (remove service, images, database, and user):**
  ```bash
  sudo apt purge photoframe
  ```

---

## 5. Using the Web Interfaces

Once the service is running, two web interfaces are available:

### 1. Frame Display (`/frame`)

- **URL:** `http://<raspberry-pi-ip>:8181/frame` (or `http://localhost:8181/frame` locally)
- Designed for display screens and digital photo frames.
- Runs fullscreen with a black background, rotates through images automatically, and turns black during night mode.
- Recovers gracefully from network interruptions and automatically syncs within seconds when new photos are uploaded or settings are changed.

### 2. Admin Dashboard (`/admin`)

- **URL:** `http://<raspberry-pi-ip>:8181/admin`
- **Upload Photos:** Drag and drop or browse files to upload multiple photos at once. Supported extensions: `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp`, `.bmp`.
- **Reorder Slideshow:** Drag and drop image tiles in the visual grid into your desired order, then click **"Save order"**.
- **Delete Images:** Click the **"Remove"** button below any image to delete it from disk and SQLite.
- **Adjust Settings:** Change the slideshow interval (in seconds) or night mode schedule (`HH:MM`) directly in the browser and click **"Save settings"**.

> [!NOTE]
> PhotoFrame is designed for trusted local home networks (LAN/Wi-Fi) and does not require authentication. If deploying to an untrusted network, place it behind a reverse proxy (e.g. Nginx, Caddy) with authentication enabled.

---

# Developer Guide

## Architecture & Tech Stack

PhotoFrame is engineered for high reliability, minimal footprint, and zero operational friction on embedded hardware:

- **Rust (2024 Edition):** Zero garbage collection pauses, predictable low memory consumption (~10–20 MB RSS), and strict memory safety.
- **Axum 0.8 & Tokio:** Modern, high-performance async HTTP framework and multithreaded runtime with streaming multipart file upload support and graceful shutdown.
- **Embedded SQLite (`rusqlite`):** Compiled with the `bundled` feature. SQLite is statically linked into the binary — no external database server or system SQLite shared library version mismatches.
- **WAL Mode & Pragmas:** Configured with `PRAGMA journal_mode = WAL;`, `PRAGMA busy_timeout = 5000;`, and `PRAGMA foreign_keys = ON;` for robust concurrency between reader endpoints and write transactions.
- **Zero-Build Frontend:** Self-contained vanilla HTML5, CSS3, and modern JavaScript embedded directly into the Rust binary as static strings. No Node.js, npm, webpack, or bundler toolchains required to develop or build.
- **Systemd Hardening:** Service definition incorporates Linux sandboxing flags (`ProtectSystem=strict`, `ProtectHome=true`, `NoNewPrivileges=true`, `PrivateTmp=true`, restricted `ReadWritePaths`).

---

## Repository & Code Structure

```text
photoframe/
├── Cargo.toml                       # Root Cargo workspace manifest
├── Cargo.lock                       # Pinned dependency tree
├── config.example.yaml              # Reference configuration template
├── config.yaml                      # Local development configuration (gitignored)
├── packaging/                       # Debian packaging definitions
│   └── debian/
│       ├── control.in               # Debian package control template (__VERSION__, __ARCH__)
│       ├── conffiles                # Declares /etc/photoframe/config.yaml as a preserved conffile
│       ├── photoframe.service       # Systemd unit file with security sandboxing
│       ├── postinst                 # Creates 'photoframe' system user, creates dirs, enables service
│       ├── prerm                    # Stops and disables service before removal
│       └── postrm                   # Cleans up files and removes system user on purge
├── scripts/
│   └── build-deb.sh                 # Cross-platform Debian package builder using dpkg-deb / zigbuild
├── .github/
│   └── workflows/
│       └── release.yml              # GitHub Actions automated release pipeline for ARMv7
└── crates/
    └── photoframe-service/          # Main application crate
        ├── Cargo.toml               # Crate manifest & dependencies
        └── src/
            ├── main.rs              # App entrypoint, preflight path checks, Axum routing, shutdown signals
            ├── config.rs            # Hierarchical config resolution (CLI, Env, File, Defaults) & validation
            ├── db.rs                # SQLite initialization, schema migrations, atomic queries & reorder logic
            ├── admin.rs             # Admin UI HTML & endpoints (upload, delete, reorder, settings)
            └── frame.rs             # Fullscreen frame HTML & state polling endpoint (night mode logic)
```

### Module Responsibilities

- **`main.rs`:** Orchestrates initialization. Runs `ensure_runtime_paths()` to verify that image and database directories exist and are writable before opening ports. Mounts all routes, applies request body size limits (100MB max upload), and manages graceful shutdown on `SIGINT` (Ctrl+C) and `SIGTERM`.
- **`config.rs`:** Defines `AppConfig` and loads settings through the precedence chain: built-in defaults $\rightarrow$ YAML file $\rightarrow$ environment variables $\rightarrow$ CLI arguments. Validates ranges, ports, and `HH:MM` time strings using `chrono::NaiveTime`.
- **`db.rs`:** Manages SQLite connection and schema migration runner. Implements transactional image insertion, safe image deletion with automatic `sort_index` gap compaction, and atomic multi-image reordering using temporary index offsets.
- **`admin.rs`:** Serves the responsive administration dashboard (`/admin`) and handles administrative API routes:
  - Validates file extensions (`jpg`, `jpeg`, `png`, `gif`, `webp`, `bmp`) and generates timestamp-prefixed safe filenames.
  - Serves cached thumbnail images.
  - Implements image reordering and persistent settings updates.
- **`frame.rs`:** Serves the slideshow view (`/frame`) and `/frame/api/state`. Computes whether the current local time falls within the configured night-mode window (including overnight windows like `20:00` - `06:00`).

---

## Local Development Setup

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable channel, 1.85+ recommended for the 2024 edition)

### Running Locally

1. **Clone the repository:**

   ```bash
   git clone https://github.com/dArignac/photoframe.git
   cd photoframe
   ```

2. **Prepare a local configuration file:**
   Create a local `config.yaml` with paths inside the repository directory so root privileges are not required:

   ```bash
   cp config.example.yaml config.yaml
   ```

   Edit `config.yaml`:

   ```yaml
   bind_address: 127.0.0.1
   port: 8080
   image_dir: ./images
   database_path: ./photoframe.sqlite
   slideshow_interval_seconds: 10
   night_mode_start: "22:00"
   night_mode_end: "06:00"
   frame_poll_interval_seconds: 5
   display_fit_mode: contain
   ```

3. **Run the service:**

   ```bash
   cargo run -p photoframe-service -- --config ./config.yaml
   ```

   To enable verbose debug logs:

   ```bash
   RUST_LOG=debug cargo run -p photoframe-service -- --config ./config.yaml
   ```

4. **Access the application in your browser:**
   - **Admin UI:** [http://localhost:8080/admin](http://localhost:8080/admin)
   - **Frame Display:** [http://localhost:8080/frame](http://localhost:8080/frame)
   - **Health Endpoint:** [http://localhost:8080/health](http://localhost:8080/health)

---

## Testing & Quality Checks

Run the automated test suite covering DB operations, night mode time windows, config parsing, and HTTP routes:

```bash
# Run all unit and integration tests
cargo test

# Run Clippy linter
cargo clippy --all-targets -- -D warnings

# Check code formatting
cargo fmt --check
```

---

## Building the Debian Package

The `scripts/build-deb.sh` script automates compiling the binary, creating the staging root filesystem layout, injecting maintainer scripts, and packaging with `dpkg-deb`.

### Native Build (Host Architecture)

Prerequisites: `cargo`, `dpkg`, and `dpkg-deb`.

```bash
./scripts/build-deb.sh 0.1.0
```

The output package will be placed in `target/package/photoframe_0.1.0_<arch>.deb`.

### Cross-Compiling for Raspberry Pi

The project supports building ARM packages using [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) without needing cross-compilation GCC toolchains:

1. **Install Zig and cargo-zigbuild:**

   ```bash
   pip install cargo-zigbuild ziglang
   ```

2. **Add the target architecture to Rust:**

   ```bash
   # For 32-bit Raspberry Pi OS (ARMv7 / armhf):
   rustup target add armv7-unknown-linux-gnueabihf

   # For 64-bit Raspberry Pi OS (ARM64 / aarch64):
   rustup target add aarch64-unknown-linux-gnu
   ```

3. **Build the package:**

   ```bash
   # Build for ARMv7 (with glibc 2.31 compatibility):
   TARGET=armv7-unknown-linux-gnueabihf.2.31 ./scripts/build-deb.sh 0.1.0

   # Build for ARM64:
   TARGET=aarch64-unknown-linux-gnu ./scripts/build-deb.sh 0.1.0
   ```

---

## Database Schema & Migrations

PhotoFrame maintains an embedded SQLite database (`photoframe.sqlite`). Migrations are managed in `crates/photoframe-service/src/db.rs` and run automatically inside an atomic transaction on startup.

### Schema Structure

```sql
-- Migration tracking
CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY NOT NULL
);

-- Persistent settings overrides configured via admin UI
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Uploaded image metadata and ordering
CREATE TABLE IF NOT EXISTS images (
    id INTEGER PRIMARY KEY,
    file_name TEXT NOT NULL,
    sort_index INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_images_sort_index ON images(sort_index);
```

### Key DB Invariants

- **Gapless Sequential Indexing:** Image items maintain an ordered sequence `sort_index = 0, 1, 2, ... N-1`.
- **Atomic Compaction:** When an image is deleted, all images with higher `sort_index` values are decremented by 1 within the same transaction to maintain gapless ordering.
- **Two-Phase Reordering:** To prevent unique index collisions (`idx_images_sort_index`) during reordering, IDs are shifted by an offset window in the first step and assigned their target indices in the second step before committing.

---

## HTTP API Reference

All APIs return standard HTTP status codes and JSON error bodies `{"error": "message"}` on failure.

| Method   | Endpoint                       | Description                                             | Request Body                                                                                 | Response Body                                                  |
| -------- | ------------------------------ | ------------------------------------------------------- | -------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `GET`    | `/health`                      | Health and liveness check                               | None                                                                                         | `"ok"` (`text/plain`)                                          |
| `GET`    | `/frame`                       | Fullscreen photo slideshow UI                           | None                                                                                         | HTML                                                           |
| `GET`    | `/frame/api/state`             | Current frame state and images payload                  | None                                                                                         | `FrameStatePayload` (JSON)                                     |
| `GET`    | `/images/{file_name}`          | Serves image file for slideshow                         | None                                                                                         | Image binary (`image/jpeg`, etc.) with immutable cache headers |
| `GET`    | `/admin`                       | Web administration UI                                   | None                                                                                         | HTML                                                           |
| `GET`    | `/admin/images/{file_name}`    | Serves image thumbnail for admin UI                     | None                                                                                         | Image binary                                                   |
| `GET`    | `/admin/api/images`            | List all images ordered by `sort_index`                 | None                                                                                         | `Vec<StoredImage>` (JSON)                                      |
| `POST`   | `/admin/api/upload`            | Upload one or more image files                          | `multipart/form-data` with `image` file fields                                               | `Vec<StoredImage>` (JSON) of newly uploaded items              |
| `DELETE` | `/admin/api/images/{image_id}` | Deletes image from disk and database                    | None                                                                                         | `204 No Content`                                               |
| `POST`   | `/admin/api/reorder`           | Update display order of all images                      | `{"ordered_ids": [3, 1, 2]}`                                                                 | `204 No Content`                                               |
| `GET`    | `/admin/api/settings`          | Read current slideshow interval and night mode settings | None                                                                                         | `AdminSettingsPayload` (JSON)                                  |
| `POST`   | `/admin/api/settings`          | Save slideshow interval and night mode settings         | `{"slideshow_interval_seconds": 30, "night_mode_start": "20:00", "night_mode_end": "06:00"}` | `204 No Content`                                               |

### Sample JSON Payloads

#### `GET /frame/api/state`

```json
{
  "slideshow_interval_seconds": 30,
  "frame_poll_interval_seconds": 15,
  "display_fit_mode": "contain",
  "night_mode_active": false,
  "images": [
    {
      "id": 1,
      "file_name": "1726839210000000000-sample.jpg",
      "url": "/images/1726839210000000000-sample.jpg"
    }
  ]
}
```

#### `GET /admin/api/settings`

```json
{
  "slideshow_interval_seconds": 30,
  "night_mode_start": "20:00",
  "night_mode_end": "06:00"
}
```

---

## License

This project is open source and available under the terms of the repository license.
