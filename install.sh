#!/bin/sh

set -e

# Temporary setup
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
TARGET=""

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64) TARGET="linux-amd64" ;;
            aarch64|arm64) TARGET="linux-arm64" ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64) TARGET="macos-amd64" ;;
            aarch64|arm64) TARGET="macos-arm64" ;;
        esac
        ;;
esac

BINARY_NAME=""

if [ -n "$TARGET" ]; then
    echo "==> Downloading pre-compiled binary for $TARGET..."
    URL="https://github.com/JoeriKaiser/pin/releases/latest/download/pin-$TARGET"
    if curl -fsSL "$URL" -o "$TEMP_DIR/pin"; then
        BINARY_NAME="pin"
    else
        echo "==> Pre-compiled binary not found or download failed. Falling back to source compilation..."
    fi
fi

if [ -z "$BINARY_NAME" ]; then
    echo "==> Downloading main.zig..."
    curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/main.zig -o "$TEMP_DIR/main.zig"

    echo "==> Compiling pin CLI with Zig..."
    if ! command -v zig >/dev/null 2>&1; then
        echo "Error: Zig compiler not found and no pre-compiled binary available for $OS-$ARCH." >&2
        echo "Please install Zig (https://ziglang.org) to build from source, or check releases at https://github.com/JoeriKaiser/pin/releases." >&2
        exit 1
    fi

    (
        cd "$TEMP_DIR"
        zig build-exe main.zig -O ReleaseSafe
    )
    BINARY_NAME="main"
fi
INSTALL_PATH="/usr/local/bin/pin"
LOCAL_BIN_DIR="$HOME/.local/bin"

echo "==> Installing pin binary..."
if [ -w "/usr/local/bin" ]; then
    cp "$TEMP_DIR/$BINARY_NAME" "$INSTALL_PATH"
    chmod +x "$INSTALL_PATH"
    echo "Installed pin to $INSTALL_PATH"
else
    mkdir -p "$LOCAL_BIN_DIR"
    cp "$TEMP_DIR/$BINARY_NAME" "$LOCAL_BIN_DIR/pin"
    chmod +x "$LOCAL_BIN_DIR/pin"
    echo "Installed pin to $LOCAL_BIN_DIR/pin"

    # Check if ~/.local/bin is in PATH
    case ":$PATH:" in
        *":$LOCAL_BIN_DIR:"*) ;;
        *)
            echo "Warning: $LOCAL_BIN_DIR is not in your PATH. You might need to add it to your shell config."
            ;;
    esac
fi

echo "==> Installation complete! Run 'pin' to get started."
