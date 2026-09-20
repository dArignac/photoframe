# PhotoFrame

Bootstrapped Rust service for a Raspberry Pi photo frame application.

## Run

Copy the `config.example.yaml` to `config.yaml` and adjust the values if necessary.

```bash
cargo run -p photoframe-service -- --config ./config.yaml
```

Logging defaults to `info` if `RUST_LOG` is unset. To increase verbosity, for example:

```bash
RUST_LOG=debug cargo run -p photoframe-service -- --config ./config.yaml
```

The service starts with:

- `GET /health` basic liveness endpoint (`ok`)
- `GET /admin` admin UI (upload, reorder, settings)
- `GET /frame` fullscreen frame UI with automatic rotation
- `GET /frame/api/state` frame state/config payload (images, interval, fit mode, night-mode active flag)
- `GET /admin/api/images` list images
- `DELETE /admin/api/images/{image_id}` remove an image and close ordering gaps
- `GET /images/{file_name}` serve uploaded image files for frame rendering
- `GET /admin/images/{file_name}` serve uploaded image files for admin thumbnails
- `POST /admin/api/upload` upload one or more images via multipart field `image` (repeat field for each file)
- `POST /admin/api/reorder` reorder images with JSON `{ "ordered_ids": [..] }`
- `GET /admin/api/settings` read effective slideshow/night settings
- `POST /admin/api/settings` persist slideshow/night settings

On startup, the service initializes SQLite and runs schema migrations for:

- `settings(key TEXT PRIMARY KEY, value TEXT NOT NULL)`
- `images(id INTEGER PRIMARY KEY, file_name TEXT NOT NULL, sort_index INTEGER NOT NULL, created_at TEXT NOT NULL)`

Before starting the HTTP server, runtime storage paths are validated:

- `image_dir` is created if missing and must be writable
- parent directory of `database_path` is created if missing and must be writable
- `database_path` must be a file path (not a directory)

## Configuration precedence

1. Defaults built into the binary
2. YAML file (`--config`, default `/etc/photoframe/config.yaml`) if it exists
3. Environment variables (`PHOTOFRAME_*`)
4. CLI flags (`--bind-address`, `--port`)

Supported env vars:

- `PHOTOFRAME_BIND_ADDRESS`
- `PHOTOFRAME_PORT`
- `PHOTOFRAME_IMAGE_DIR`
- `PHOTOFRAME_DB_PATH`
- `PHOTOFRAME_SLIDESHOW_INTERVAL_SECONDS`
- `PHOTOFRAME_NIGHT_MODE_START`
- `PHOTOFRAME_NIGHT_MODE_END`
- `PHOTOFRAME_FRAME_POLL_INTERVAL_SECONDS`
- `PHOTOFRAME_DISPLAY_FIT_MODE` (`contain` or `cover`)

Frame clients poll state/config at least every 60 seconds (or faster if configured), so slideshow interval and night-mode setting changes are applied within that window.

## Debian packaging and service

Build a Debian package:

```bash
./scripts/build-deb.sh 0.1.0
```

The package installs:

- `/usr/bin/photoframe-service`
- `/etc/photoframe/config.yaml` (conffile)
- `/etc/photoframe/images/`
- `/lib/systemd/system/photoframe.service`

Package maintainer scripts will enable and (re)start `photoframe.service` on install/configure, stop/disable it on remove, and clean up the images folder on uninstall.
