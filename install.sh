#!/bin/sh

set -e

# Temporary setup
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

echo "==> Downloading main.zig..."
curl -fsSL https://raw.githubusercontent.com/JoeriKaiser/pin/main/main.zig -o "$TEMP_DIR/main.zig"

echo "==> Compiling pin CLI with Zig..."
if ! command -v zig >/dev/null 2>&1; then
    echo "Error: Zig compiler not found. Please install Zig (https://ziglang.org) and try again." >&2
    exit 1
fi

(
    cd "$TEMP_DIR"
    zig build-exe main.zig -O ReleaseSafe
)

BINARY_NAME="main"
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
