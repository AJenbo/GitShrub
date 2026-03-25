#!/bin/sh
set -e

PREFIX="${PREFIX:-/usr/local}"
BIN_DIR="$PREFIX/bin"
ICON_DIR="$PREFIX/share/icons/hicolor/256x256/apps"
DESKTOP_DIR="$PREFIX/share/applications"

if [ ! -f target/release/gitshrub ]; then
    echo "No release binary found. Build it first as your normal user:"
    echo ""
    echo "  cargo build --release"
    echo ""
    echo "Then re-run this script."
    exit 1
fi

echo "Installing GitShrub to $PREFIX ..."

# Install binary
install -Dm755 target/release/gitshrub "$BIN_DIR/gitshrub"
echo "  Installed binary to $BIN_DIR/gitshrub"

# Install icon
install -Dm644 icon.png "$ICON_DIR/gitshrub.png"
echo "  Installed icon to $ICON_DIR/gitshrub.png"

# Install desktop entry
install -Dm644 gitshrub.desktop "$DESKTOP_DIR/gitshrub.desktop"
echo "  Installed desktop entry to $DESKTOP_DIR/gitshrub.desktop"

# Update icon cache if available
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
    echo "  Updated icon cache"
fi

# Update desktop database if available
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
    echo "  Updated desktop database"
fi

echo "Done. You may need to log out and back in for the dock icon to appear."