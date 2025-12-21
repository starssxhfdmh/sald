#!/bin/bash
# Sald Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/starssxhfdmh/sald/master/install.sh | bash

set -e

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
DIM='\033[2m'
RESET='\033[0m'

REPO="starssxhfdmh/sald"
INSTALL_DIR="$HOME/.sald"
BIN_DIR="$INSTALL_DIR/bin"
TEMP_DIR="/tmp/sald-install-$$"

# Detect OS and architecture with proper priority
detect_os() {
    OS=""
    EXT=""
    ARCH=""
    
    # Detect architecture first
    case "$(uname -m)" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        aarch64|arm64)
            ARCH="arm64"
            ;;
        *)
            echo -e "  ${RED}Unsupported architecture: $(uname -m)${RESET}"
            exit 1
            ;;
    esac
    
    # Detect OS with proper priority
    # Check Windows environments first (highest priority)
    if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        OS="windows"
        EXT=".exe"
    elif [ -n "$MSYSTEM" ]; then
        # MSYS2 environment (Git Bash on Windows)
        OS="windows"
        EXT=".exe"
    elif uname -r | grep -qi microsoft; then
        # WSL (Windows Subsystem for Linux)
        # WSL should use Linux binaries, not Windows
        OS="linux"
        EXT=""
    else
        # Pure Unix-like systems
        case "$(uname -s)" in
            Linux*)
                OS="linux"
                EXT=""
                ;;
            Darwin*)
                OS="darwin"
                EXT=""
                # Check if macOS is supported
                echo -e "  ${RED}macOS is not yet supported${RESET}"
                exit 1
                ;;
            FreeBSD*)
                echo -e "  ${RED}FreeBSD is not yet supported${RESET}"
                exit 1
                ;;
            *)
                echo -e "  ${RED}Unsupported OS: $(uname -s)${RESET}"
                exit 1
                ;;
        esac
    fi
    
    # Verify detection
    if [ -z "$OS" ] || [ -z "$ARCH" ]; then
        echo -e "  ${RED}Failed to detect OS or architecture${RESET}"
        echo -e "  ${DIM}OS: $(uname -s), Arch: $(uname -m)${RESET}"
        echo -e "  ${DIM}OSTYPE: ${OSTYPE:-not set}${RESET}"
        exit 1
    fi
}

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

    # Detect OS
    detect_os
    echo -e "  ${DIM}Platform: ${OS}-${ARCH}${RESET}"

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
    SUFFIX="${OS}-${ARCH}${EXT}"
    
    # Download binaries with progress
    echo -e "  ${DIM}Downloading...${RESET}"
    
    download_with_progress "$BASE_URL/sald-${SUFFIX}" "$TEMP_DIR/sald${EXT}" "sald" 0 3
    download_with_progress "$BASE_URL/sald-lsp-${SUFFIX}" "$TEMP_DIR/sald-lsp${EXT}" "sald-lsp" 1 3
    download_with_progress "$BASE_URL/salad-${SUFFIX}" "$TEMP_DIR/salad${EXT}" "salad" 2 3
    
    clear_line
    echo -e "  ${GREEN}Downloaded${RESET} ${DIM}3 binaries${RESET}"

    # Make executable (not needed for Windows but doesn't hurt)
    chmod +x "$TEMP_DIR/sald${EXT}" "$TEMP_DIR/sald-lsp${EXT}" "$TEMP_DIR/salad${EXT}" 2>/dev/null || true

    # Create install directory
    mkdir -p "$BIN_DIR"

    # Move binaries
    echo -e "  ${DIM}Installing...${RESET}"
    mv "$TEMP_DIR/sald${EXT}" "$BIN_DIR/sald${EXT}"
    mv "$TEMP_DIR/sald-lsp${EXT}" "$BIN_DIR/sald-lsp${EXT}"
    mv "$TEMP_DIR/salad${EXT}" "$BIN_DIR/salad${EXT}"
    clear_line
    echo -e "  ${GREEN}Installed${RESET} ${DIM}to $BIN_DIR${RESET}"

    # Add to PATH (only for non-Windows)
    if [ "$OS" != "windows" ]; then
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
    fi

    # Success message
    echo ""
    echo -e "${GREEN}Done${RESET}"
    echo ""
    
    if [ "$OS" = "windows" ]; then
        echo -e "  ${DIM}Add to PATH: $BIN_DIR${RESET}"
        echo -e "  ${DIM}Or run directly: $BIN_DIR/sald${EXT}${RESET}"
    elif [ -n "$SHELL_CONFIG" ]; then
        echo -e "  ${DIM}Run: source $SHELL_CONFIG${RESET}"
        echo -e "  ${DIM}Or restart your terminal${RESET}"
    else
        echo -e "  ${DIM}Add to PATH: export PATH=\"\$HOME/.sald/bin:\$PATH\"${RESET}"
    fi
    echo ""
}

main