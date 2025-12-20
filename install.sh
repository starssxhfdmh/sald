#!/bin/bash
# Sald Installer for Linux
# Usage: curl -fsSL https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.sh | bash

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

# Progress bar
progress_bar() {
    local current=$1
    local total=$2
    local name=$3
    local width=30
    local percent=$((current * 100 / total))
    local filled=$((current * width / total))
    local empty=$((width - filled))
    
    printf "\r  ${DIM}[${RESET}"
    printf "%${filled}s" | tr ' ' '='
    printf "%${empty}s" | tr ' ' ' '
    printf "${DIM}]${RESET} %3d%% ${CYAN}%s${RESET}" "$percent" "$name"
}

# Clear line
clear_line() {
    printf "\r\033[K"
}

# Get latest release tag
get_latest_version() {
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
}

# Download with progress
download_with_progress() {
    local url="$1"
    local output="$2"
    local name="$3"
    local step="$4"
    local total="$5"
    
    progress_bar $step $total "$name"
    curl -fsSL "$url" -o "$output" 2>/dev/null
    clear_line
    progress_bar $((step + 1)) $total "$name"
}

main() {
    echo ""
    echo -e "${GREEN}sald${RESET} installer"
    echo ""

    # Get latest version
    echo -e "  ${DIM}Fetching latest version...${RESET}"
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        echo -e "  ${RED}Failed to get latest version${RESET}"
        exit 1
    fi
    clear_line
    echo -e "  ${DIM}Version: ${VERSION}${RESET}"
    echo ""

    # Create temp directory
    mkdir -p "$TEMP_DIR"
    trap "rm -rf $TEMP_DIR" EXIT

    BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
    
    # Download binaries with progress
    echo -e "  ${DIM}Downloading...${RESET}"
    
    download_with_progress "$BASE_URL/sald-linux-x86_64" "$TEMP_DIR/sald" "sald" 0 3
    download_with_progress "$BASE_URL/sald-lsp-linux-x86_64" "$TEMP_DIR/sald-lsp" "sald-lsp" 1 3
    download_with_progress "$BASE_URL/salad-linux-x86_64" "$TEMP_DIR/salad" "salad" 2 3
    
    clear_line
    echo -e "  ${GREEN}Downloaded${RESET} ${DIM}3 binaries${RESET}"

    # Make executable
    chmod +x "$TEMP_DIR/sald" "$TEMP_DIR/sald-lsp" "$TEMP_DIR/salad"

    # Create install directory
    mkdir -p "$BIN_DIR"

    # Move binaries
    echo -e "  ${DIM}Installing...${RESET}"
    mv "$TEMP_DIR/sald" "$BIN_DIR/sald"
    mv "$TEMP_DIR/sald-lsp" "$BIN_DIR/sald-lsp"
    mv "$TEMP_DIR/salad" "$BIN_DIR/salad"
    clear_line
    echo -e "  ${GREEN}Installed${RESET} ${DIM}to $BIN_DIR${RESET}"

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
            echo -e "  ${GREEN}Updated${RESET} ${DIM}$SHELL_CONFIG${RESET}"
        fi
    fi

    # Success message
    echo ""
    echo -e "${GREEN}Done${RESET}"
    echo ""
    
    if [ -n "$SHELL_CONFIG" ]; then
        echo -e "  ${DIM}Run: source $SHELL_CONFIG${RESET}"
        echo -e "  ${DIM}Or restart your terminal${RESET}"
    else
        echo -e "  ${DIM}Add to PATH: export PATH=\"\$HOME/.sald/bin:\$PATH\"${RESET}"
    fi
    echo ""
}

main
