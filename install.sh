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

# Format file size for display
format_size() {
    local size=$1
    if [ "$size" -ge 1048576 ]; then
        # Use awk instead of bc for better compatibility
        awk "BEGIN {printf \"%.1f MB\", $size / 1048576}"
    elif [ "$size" -ge 1024 ]; then
        printf "%d KB" "$((size / 1024))"
    else
        printf "%d B" "$size"
    fi
}

# Format speed
format_speed() {
    local speed=$1
    if [ "$speed" -ge 1048576 ]; then
        awk "BEGIN {printf \"%.1f MB/s\", $speed / 1048576}"
    elif [ "$speed" -ge 1024 ]; then
        printf "%d KB/s" "$((speed / 1024))"
    else
        printf "%d B/s" "$speed"
    fi
}

# Format ETA
format_eta() {
    local seconds=$1
    if [ -z "$seconds" ] || [ "$seconds" -lt 0 ] 2>/dev/null || [ "$seconds" -gt 3600 ] 2>/dev/null; then
        echo -n "--:--"
    elif [ "$seconds" -ge 60 ]; then
        echo -n "${seconds}s"
        # printf "%dm %ds" "$((seconds / 60))" "$((seconds % 60))"
    else
        echo -n "${seconds}s"
    fi
}

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
    if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        OS="windows"
        EXT=".exe"
    elif [ -n "$MSYSTEM" ]; then
        OS="windows"
        EXT=".exe"
    elif uname -r | grep -qi microsoft; then
        OS="linux"
        EXT=""
    else
        case "$(uname -s)" in
            Linux*)
                OS="linux"
                EXT=""
                ;;
            Darwin*)
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
    
    if [ -z "$OS" ] || [ -z "$ARCH" ]; then
        echo -e "  ${RED}Failed to detect OS or architecture${RESET}"
        exit 1
    fi
}

# Clear line
clear_line() {
    printf "\r\033[K"
}

# Download with real-time progress, speed, and ETA
download_with_progress() {
    local url="$1"
    local output="$2"
    local name="$3"
    local expected_size="$4"
    
    local start_time=$(date +%s)
    local last_time=$start_time
    local last_bytes=0
    local speed=0
    
    # Start download in background
    curl -fsSL "$url" -o "$output" 2>/dev/null &
    local pid=$!
    
    # Show progress while downloading
    while kill -0 $pid 2>/dev/null; do
        if [ -f "$output" ]; then
            local current_size=$(stat -c%s "$output" 2>/dev/null || stat -f%z "$output" 2>/dev/null || echo 0)
            local now=$(date +%s)
            local elapsed=$((now - last_time))
            
            if [ "$elapsed" -ge 1 ]; then
                speed=$(( (current_size - last_bytes) / elapsed ))
                last_bytes=$current_size
                last_time=$now
            fi
            
            local percent=0
            local filled=0
            local width=30
            
            if [ "$expected_size" -gt 0 ]; then
                percent=$((current_size * 100 / expected_size))
                filled=$((current_size * width / expected_size))
            fi
            local empty=$((width - filled))
            
            local remaining=-1
            if [ "$speed" -gt 0 ] && [ "$expected_size" -gt 0 ]; then
                remaining=$(( (expected_size - current_size) / speed ))
            fi
            
            local size_str=$(format_size "$current_size")
            local total_str=$(format_size "$expected_size")
            local speed_str=$(format_speed "$speed")
            local eta_str=$(format_eta "$remaining")
            
            printf "\r  ${DIM}[${RESET}"
            printf "%${filled}s" | tr ' ' '='
            printf "%${empty}s"
            printf "${DIM}]${RESET} %3d%% ${CYAN}%s${RESET} ${DIM}(%s/%s @ %s, ETA %s)${RESET}    " \
                "$percent" "$name" "$size_str" "$total_str" "$speed_str" "$eta_str"
        fi
        sleep 0.1
    done
    
    # Wait for download to complete
    wait $pid || {
        echo -e "\r  ${RED}Failed to download $name${RESET}"
        return 1
    }
    
    # Final progress
    if [ -f "$output" ]; then
        local final_size=$(stat -c%s "$output" 2>/dev/null || stat -f%z "$output" 2>/dev/null || echo "$expected_size")
        local end_time=$(date +%s)
        local total_time=$((end_time - start_time))
        if [ "$total_time" -eq 0 ]; then total_time=1; fi
        local avg_speed=$((final_size / total_time))
        
        local size_str=$(format_size "$final_size")
        local speed_str=$(format_speed "$avg_speed")
        
        printf "\r  ${DIM}[${RESET}"
        printf "%30s" | tr ' ' '='
        printf "${DIM}]${RESET} 100%% ${CYAN}%s${RESET} ${DIM}(%s @ %s)${RESET}              \n" \
            "$name" "$size_str" "$speed_str"
    fi
    
    return 0
}

main() {
    echo ""
    echo -e "${GREEN}sald${RESET} installer"
    echo ""

    # Detect OS
    detect_os
    echo -e "  ${DIM}Platform: ${OS}-${ARCH}${RESET}"

    # Get latest release info
    printf "  ${DIM}Fetching latest version...${RESET}"
    RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest")
    VERSION=$(echo "$RELEASE_JSON" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
    
    if [ -z "$VERSION" ]; then
        echo ""
        echo -e "  ${RED}Failed to get latest version${RESET}"
        exit 1
    fi
    clear_line
    echo -e "  ${DIM}Version: ${VERSION}${RESET}"
    echo ""

    # Create temp directory
    mkdir -p "$TEMP_DIR"
    trap "rm -rf $TEMP_DIR" EXIT

    SUFFIX="${OS}-${ARCH}${EXT}"
    
    # Asset names
    SALD_NAME="sald-${SUFFIX}"
    LSP_NAME="sald-lsp-${SUFFIX}"
    SALAD_NAME="salad-${SUFFIX}"
    
    # Extract asset info using grep and sed (more reliable than multi-line grep)
    # Get sizes
    SALD_SIZE=$(echo "$RELEASE_JSON" | tr ',' '\n' | grep -A2 "\"name\": \"$SALD_NAME\"" | grep '"size"' | head -1 | sed -E 's/[^0-9]*([0-9]+).*/\1/')
    LSP_SIZE=$(echo "$RELEASE_JSON" | tr ',' '\n' | grep -A2 "\"name\": \"$LSP_NAME\"" | grep '"size"' | head -1 | sed -E 's/[^0-9]*([0-9]+).*/\1/')
    SALAD_SIZE=$(echo "$RELEASE_JSON" | tr ',' '\n' | grep -A2 "\"name\": \"$SALAD_NAME\"" | grep '"size"' | head -1 | sed -E 's/[^0-9]*([0-9]+).*/\1/')
    
    # Use direct GitHub download URLs (more reliable than parsing browser_download_url)
    BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
    SALD_URL="$BASE_URL/$SALD_NAME"
    LSP_URL="$BASE_URL/$LSP_NAME"
    SALAD_URL="$BASE_URL/$SALAD_NAME"
    
    # Default sizes if not found (fallback)
    [ -z "$SALD_SIZE" ] && SALD_SIZE=8000000
    [ -z "$LSP_SIZE" ] && LSP_SIZE=3500000
    [ -z "$SALAD_SIZE" ] && SALAD_SIZE=8500000
    
    # Download binaries
    echo -e "  ${DIM}Downloading...${RESET}"
    
    download_with_progress "$SALD_URL" "$TEMP_DIR/sald${EXT}" "sald" "$SALD_SIZE" || exit 1
    download_with_progress "$LSP_URL" "$TEMP_DIR/sald-lsp${EXT}" "sald-lsp" "$LSP_SIZE" || exit 1
    download_with_progress "$SALAD_URL" "$TEMP_DIR/salad${EXT}" "salad" "$SALAD_SIZE" || exit 1
    
    TOTAL_SIZE=$((SALD_SIZE + LSP_SIZE + SALAD_SIZE))
    TOTAL_STR=$(format_size "$TOTAL_SIZE")
    echo -e "  ${GREEN}Downloaded${RESET} ${DIM}3 binaries ($TOTAL_STR)${RESET}"

    # Make executable
    chmod +x "$TEMP_DIR/sald${EXT}" "$TEMP_DIR/sald-lsp${EXT}" "$TEMP_DIR/salad${EXT}" 2>/dev/null || true

    # Create install directory
    mkdir -p "$BIN_DIR"

    # Move binaries
    printf "  ${DIM}Installing...${RESET}"
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