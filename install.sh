#!/bin/bash
# Sald Installer for Linux
# Usage: curl -fsSL https://raw.githubusercontent.com/starssxhfdmh/sald/main/install.sh | bash

set -e

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
DIM='\033[2m'
RESET='\033[0m'

REPO="starssxhfdmh/sald"
INSTALL_DIR="$HOME/.sald"
BIN_DIR="$INSTALL_DIR/bin"
TEMP_DIR="/tmp/sald-install-$$"

# Get latest release tag
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
}

# Download file
download() {
    local url="$1"
    local output="$2"
    curl -fsSL "$url" -o "$output"
}

main() {
    # Get latest version
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        echo "Failed to get latest version"
        exit 1
    fi

    # Create temp directory
    mkdir -p "$TEMP_DIR"
    trap "rm -rf $TEMP_DIR" EXIT

    # Download binaries
    BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
    
    download "$BASE_URL/sald-linux-x86_64" "$TEMP_DIR/sald"
    download "$BASE_URL/sald-lsp-linux-x86_64" "$TEMP_DIR/sald-lsp"
    download "$BASE_URL/salad-linux-x86_64" "$TEMP_DIR/salad"

    # Make executable
    chmod +x "$TEMP_DIR/sald" "$TEMP_DIR/sald-lsp" "$TEMP_DIR/salad"

    # Create install directory
    mkdir -p "$BIN_DIR"

    # Move binaries
    mv "$TEMP_DIR/sald" "$BIN_DIR/sald"
    mv "$TEMP_DIR/sald-lsp" "$BIN_DIR/sald-lsp"
    mv "$TEMP_DIR/salad" "$BIN_DIR/salad"

    # Add to PATH
    SHELL_CONFIG=""
    if [ -n "$ZSH_VERSION" ] || [ -f "$HOME/.zshrc" ]; then
        SHELL_CONFIG="$HOME/.zshrc"
    elif [ -n "$BASH_VERSION" ] || [ -f "$HOME/.bashrc" ]; then
        SHELL_CONFIG="$HOME/.bashrc"
    elif [ -f "$HOME/.profile" ]; then
        SHELL_CONFIG="$HOME/.profile"
    fi

    PATH_LINE='export PATH="$HOME/.sald/bin:$PATH"'
    
    if [ -n "$SHELL_CONFIG" ]; then
        if ! grep -q '.sald/bin' "$SHELL_CONFIG" 2>/dev/null; then
            echo "" >> "$SHELL_CONFIG"
            echo "# Sald" >> "$SHELL_CONFIG"
            echo "$PATH_LINE" >> "$SHELL_CONFIG"
        fi
    fi

    # Success message
    echo ""
    echo -e "${GREEN}sald${RESET}"
    echo ""
    echo -e "  Installed ${CYAN}sald${RESET}, ${CYAN}sald-lsp${RESET}, ${CYAN}salad${RESET} ${DIM}${VERSION}${RESET}"
    echo -e "  ${DIM}Location: $BIN_DIR${RESET}"
    echo ""
    
    if [ -n "$SHELL_CONFIG" ]; then
        echo -e "  ${DIM}Run: source $SHELL_CONFIG${RESET}"
        echo -e "  ${DIM}Or restart your terminal${RESET}"
    else
        echo -e "  ${DIM}Add to PATH: export PATH=\"\$HOME/.sald/bin:\$PATH\"${RESET}"
    fi
    echo ""
    echo -e "${GREEN}Done${RESET}"
    echo ""
}

main
