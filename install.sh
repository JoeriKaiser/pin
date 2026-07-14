#!/bin/sh

set -e

# Temporary setup
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

verify_checksum() {
    FILE="$1"
    SHA_FILE="$2"
    EXPECTED=$(tr -d '\r\n' < "$SHA_FILE" | cut -d' ' -f1)
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL=$(sha256sum "$FILE" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        ACTUAL=$(shasum -a 256 "$FILE" | cut -d' ' -f1)
    else
        echo "Warning: Neither sha256sum nor shasum was found. Falling back to source compilation." >&2
        return 1
    fi
    EXPECTED=$(echo "$EXPECTED" | tr -d ' ')
    ACTUAL=$(echo "$ACTUAL" | tr -d ' ')
    [ "$EXPECTED" = "$ACTUAL" ]
}

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
        if curl -fsSL "$URL.sha256" -o "$TEMP_DIR/pin.sha256"; then
            if verify_checksum "$TEMP_DIR/pin" "$TEMP_DIR/pin.sha256"; then
                BINARY_NAME="pin"
            else
                echo "Error: Checksum verification failed for pre-compiled binary." >&2
                echo "Falling back to source compilation..."
            fi
        else
            echo "Warning: Could not download checksum file. Falling back to source compilation..."
        fi
    else
        echo "==> Pre-compiled binary not found or download failed. Falling back to source compilation..."
    fi
fi

if [ -z "$BINARY_NAME" ]; then
    REQUIRED_ZIG="0.16.0"
    LATEST_URL=$(curl -fsSL -o /dev/null -w '%{url_effective}' https://github.com/JoeriKaiser/pin/releases/latest || true)
    RELEASE_TAG=${LATEST_URL##*/}
    case "$RELEASE_TAG" in
        ""|latest) RELEASE_TAG="main" ;;
    esac

    echo "==> Downloading main.zig from $RELEASE_TAG..."
    curl -fsSL "https://raw.githubusercontent.com/JoeriKaiser/pin/$RELEASE_TAG/main.zig" -o "$TEMP_DIR/main.zig"

    echo "==> Compiling pin CLI with Zig $REQUIRED_ZIG..."
    if ! command -v zig >/dev/null 2>&1; then
        echo "Error: Zig compiler not found and no pre-compiled binary available for $OS-$ARCH." >&2
        echo "Install Zig $REQUIRED_ZIG (https://ziglang.org) or use a supported release binary." >&2
        exit 1
    fi
    ACTUAL_ZIG=$(zig version)
    if [ "$ACTUAL_ZIG" != "$REQUIRED_ZIG" ]; then
        echo "Error: Source fallback requires Zig $REQUIRED_ZIG, but found $ACTUAL_ZIG." >&2
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
