#!/usr/bin/env bash
set -euo pipefail

# Build static release binary for Linux (musl) with vendored libusb
TARGET="${1:-x86_64-unknown-linux-musl}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building static binary for target: $TARGET"

# Check if running in a container or direct host
if command -v rustup >/dev/null 2>&1; then
    rustup target add "$TARGET" || true
fi

# Build release workspace
cargo build --release --target "$TARGET" -p fs80h-cli

BINARY="$REPO_ROOT/target/$TARGET/release/fs80hctl"

if [ -f "$BINARY" ]; then
    echo "==> Build successful: $BINARY"
    if command -v file >/dev/null 2>&1; then
        file "$BINARY"
    fi
    if command -v ldd >/dev/null 2>&1; then
        echo "==> Inspecting dynamic linkages:"
        ldd "$BINARY" || echo "Binary is statically linked."
    fi
else
    echo "Error: Output binary not found at $BINARY" >&2
    exit 1
fi
