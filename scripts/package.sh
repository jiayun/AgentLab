#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/package.sh [target]
# Examples:
#   ./scripts/package.sh                              # build for current platform
#   ./scripts/package.sh x86_64-pc-windows-gnu        # cross-compile for Windows (needs `cross`)
#   ./scripts/package.sh x86_64-unknown-linux-gnu     # cross-compile for Linux (needs `cross`)

TARGET="${1:-}"
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
PROJECT="agentlab"

if [ -z "$TARGET" ]; then
    # Native build
    cargo build --release
    BIN="target/release/${PROJECT}"
    PLATFORM=$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)
else
    # Cross build
    if ! command -v cross &> /dev/null; then
        echo "Error: 'cross' not found. Install with: cargo install cross"
        exit 1
    fi
    cross build --release --target "$TARGET"
    BIN="target/${TARGET}/release/${PROJECT}"
    PLATFORM="$TARGET"
    # Windows has .exe extension
    if [[ "$TARGET" == *windows* ]]; then
        BIN="${BIN}.exe"
    fi
fi

if [ ! -f "$BIN" ]; then
    echo "Error: binary not found at $BIN"
    exit 1
fi

# Package
DIST_NAME="${PROJECT}-${VERSION}-${PLATFORM}"
DIST_DIR="dist/${DIST_NAME}"

rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

cp "$BIN" "$DIST_DIR/"
cp -r static "$DIST_DIR/"

# Include sample config and docs
cp agentlab.toml.example "$DIST_DIR/"
cp README.md LICENSE-MIT LICENSE-APACHE "$DIST_DIR/" 2>/dev/null || true

# Create archive
cd dist
if [[ "$PLATFORM" == *windows* ]]; then
    zip -r "${DIST_NAME}.zip" "$DIST_NAME"
    echo "Created dist/${DIST_NAME}.zip"
else
    tar czf "${DIST_NAME}.tar.gz" "$DIST_NAME"
    echo "Created dist/${DIST_NAME}.tar.gz"
fi
