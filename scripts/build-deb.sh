#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PKG_NAME="photoframe"

VERSION="${1:-0.1.0}"
TARGET="${TARGET:-}"
COMPRESSION="${COMPRESSION:-xz}"

for cmd in cargo dpkg dpkg-deb; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "missing required command: $cmd" >&2
    exit 1
  fi
done

if [[ -n "$TARGET" ]]; then
  cargo build --release -p photoframe-service --target "$TARGET"
  BIN_PATH="$ROOT_DIR/target/$TARGET/release/photoframe-service"
else
  cargo build --release -p photoframe-service
  BIN_PATH="$ROOT_DIR/target/release/photoframe-service"
fi

if [[ ! -x "$BIN_PATH" ]]; then
  echo "compiled binary not found at $BIN_PATH" >&2
  exit 1
fi

if [[ -n "${TARGET:-}" ]]; then
  case "$TARGET" in
    aarch64-*) DEB_ARCH="${DEB_ARCH:-arm64}" ;;
    armv7*|armhf*) DEB_ARCH="${DEB_ARCH:-armhf}" ;;
    x86_64-*) DEB_ARCH="${DEB_ARCH:-amd64}" ;;
    i686-*) DEB_ARCH="${DEB_ARCH:-i386}" ;;
    *) DEB_ARCH="${DEB_ARCH:-$(dpkg --print-architecture)}" ;;
  esac
else
  DEB_ARCH="${DEB_ARCH:-$(dpkg --print-architecture)}"
fi
STAGE_ROOT="$ROOT_DIR/target/package/${PKG_NAME}_${VERSION}_${DEB_ARCH}"
rm -rf "$STAGE_ROOT"

mkdir -p \
  "$STAGE_ROOT/DEBIAN" \
  "$STAGE_ROOT/usr/bin" \
  "$STAGE_ROOT/lib/systemd/system" \
  "$STAGE_ROOT/etc/photoframe" \
  "$STAGE_ROOT/var/lib/photoframe/images"

sed -e "s/__VERSION__/${VERSION}/g" \
    -e "s/__ARCH__/${DEB_ARCH}/g" \
  "$ROOT_DIR/packaging/debian/control.in" > "$STAGE_ROOT/DEBIAN/control"

cp "$ROOT_DIR/packaging/debian/postinst" "$STAGE_ROOT/DEBIAN/postinst"
cp "$ROOT_DIR/packaging/debian/prerm" "$STAGE_ROOT/DEBIAN/prerm"
cp "$ROOT_DIR/packaging/debian/conffiles" "$STAGE_ROOT/DEBIAN/conffiles"
chmod 0755 "$STAGE_ROOT/DEBIAN/postinst" "$STAGE_ROOT/DEBIAN/prerm"
chmod 0644 "$STAGE_ROOT/DEBIAN/control" "$STAGE_ROOT/DEBIAN/conffiles"

install -m 0755 "$BIN_PATH" "$STAGE_ROOT/usr/bin/photoframe-service"
install -m 0644 "$ROOT_DIR/packaging/debian/photoframe.service" \
  "$STAGE_ROOT/lib/systemd/system/photoframe.service"
install -m 0644 "$ROOT_DIR/config.example.yaml" "$STAGE_ROOT/etc/photoframe/config.yaml"

DEB_PATH="$ROOT_DIR/target/package/${PKG_NAME}_${VERSION}_${DEB_ARCH}.deb"
DPKG_DEB_OPTS=("-Z${COMPRESSION}")
if dpkg-deb --help 2>/dev/null | grep -q -- '--root-owner-group'; then
  DPKG_DEB_OPTS+=("--root-owner-group")
fi
dpkg-deb "${DPKG_DEB_OPTS[@]}" --build "$STAGE_ROOT" "$DEB_PATH" >/dev/null
echo "built package: $DEB_PATH"
