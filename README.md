# PhotoFrame

Bootstrapped Rust service for a Raspberry Pi photo frame application.

## Run

```bash
cargo run -p photoframe-service -- --config ./config.example.yaml
```

The service starts with:

- `GET /health` basic liveness endpoint (`ok`)
- `GET /admin` bootstrap admin placeholder
- `GET /frame` bootstrap frame placeholder

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
