#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Building PSEUDO-JARVIS (release)"
cargo build --release

BIN_DIR="${JARVIS_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$BIN_DIR"

install -m 755 target/release/jarvis "$BIN_DIR/jarvis"
install -m 755 target/release/jarvisd "$BIN_DIR/jarvisd"

# Copy tray UI into vault template location
TRAY_DEST="${JARVIS_HOME:-$HOME/.jarvis}/tray"
mkdir -p "$TRAY_DEST"
cp -r tray-ui/* "$TRAY_DEST/"

echo "==> Installed to $BIN_DIR"
echo "    jarvis, jarvisd"
echo "==> Tray UI copied to $TRAY_DEST"
echo ""
echo "Next steps:"
echo "  jarvis init"
echo "  jarvis run hello-world --yes"
echo "  jarvisd --foreground   # then open http://127.0.0.1:9477/"
