#!/bin/sh
# install.sh - Install sync-devices from GitHub Releases
# Usage: curl -fsSL https://raw.githubusercontent.com/zhu1090093659/sync-devices/master/install.sh | sh

set -eu

REPO="zhu1090093659/sync-devices"
BINARY_NAME="sync-devices"
INSTALL_DIR="${SYNC_DEVICES_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${SYNC_DEVICES_VERSION:-latest}"

# Colors (disabled if not a terminal)
if [ -t 1 ]; then
    GREEN='\033[0;32m'
    RED='\033[0;31m'
    YELLOW='\033[0;33m'
    BOLD='\033[1m'
    RESET='\033[0m'
else
    GREEN='' RED='' YELLOW='' BOLD='' RESET=''
fi

info()  { printf "${GREEN}info:${RESET} %s\n" "$1"; }
warn()  { printf "${YELLOW}warn:${RESET} %s\n" "$1"; }
error() { printf "${RED}error:${RESET} %s\n" "$1" >&2; exit 1; }

# Cleanup on exit
TMPFILE=""
cleanup() { [ -n "$TMPFILE" ] && rm -f "$TMPFILE"; }
trap cleanup EXIT

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       error "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)   echo "x86_64" ;;
        aarch64|arm64)  echo "aarch64" ;;
        *)              error "Unsupported architecture: $(uname -m). Only x86_64 and aarch64 are supported." ;;
    esac
}

# Find a download command (curl or wget)
detect_downloader() {
    if command -v curl >/dev/null 2>&1; then
        echo "curl"
    elif command -v wget >/dev/null 2>&1; then
        echo "wget"
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Download a URL to a file
download() {
    url="$1"
    dest="$2"
    downloader="$(detect_downloader)"
    if [ "$downloader" = "curl" ]; then
        curl -fsSL --retry 3 -o "$dest" "$url"
    else
        wget -q -O "$dest" "$url"
    fi
}

# Fetch a URL and print to stdout
fetch() {
    url="$1"
    downloader="$(detect_downloader)"
    if [ "$downloader" = "curl" ]; then
        curl -fsSL --retry 3 "$url"
    else
        wget -q -O- "$url"
    fi
}

# Resolve latest version from GitHub API
resolve_version() {
    if [ "$VERSION" = "latest" ]; then
        info "Fetching latest release version..."
        api_response=$(fetch "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || echo "")
        if echo "$api_response" | grep -q '"Not Found"'; then
            error "No releases found for ${REPO}. The project may not have published a release yet.\n       Check https://github.com/${REPO}/releases"
        fi
        VERSION=$(echo "$api_response" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')
        if [ -z "$VERSION" ]; then
            error "Failed to determine latest version. Set SYNC_DEVICES_VERSION to install a specific version."
        fi
    fi
}

main() {
    os="$(detect_os)"
    arch="$(detect_arch)"
    artifact="${BINARY_NAME}-${os}-${arch}"

    printf "\n${BOLD}sync-devices installer${RESET}\n\n"
    info "Detected platform: ${os}/${arch}"

    resolve_version
    info "Installing version: ${VERSION}"

    url="https://github.com/${REPO}/releases/download/${VERSION}/${artifact}"
    info "Downloading ${url}"

    TMPFILE="$(mktemp)"
    download "$url" "$TMPFILE" || error "Download failed. Check that version ${VERSION} exists and has a binary for ${os}/${arch}."

    # Verify non-empty download
    if [ ! -s "$TMPFILE" ]; then
        error "Downloaded file is empty. The release may not have a binary for ${os}/${arch}."
    fi

    chmod +x "$TMPFILE"

    # Remove macOS quarantine attribute
    if [ "$os" = "darwin" ]; then
        xattr -d com.apple.quarantine "$TMPFILE" 2>/dev/null || true
    fi

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "$TMPFILE" "${INSTALL_DIR}/${BINARY_NAME}"
    TMPFILE=""
    info "Installed to ${INSTALL_DIR}/${BINARY_NAME}"

    # Verify
    if "${INSTALL_DIR}/${BINARY_NAME}" --version >/dev/null 2>&1; then
        installed_ver="$("${INSTALL_DIR}/${BINARY_NAME}" --version)"
        info "Verified: ${installed_ver}"
    else
        warn "Binary installed but verification failed. You may need to check compatibility."
    fi

    # PATH guidance
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            printf "\n"
            warn "${INSTALL_DIR} is not in your PATH."
            printf "\n  Add it by running:\n\n"
            case "${SHELL:-/bin/sh}" in
                */zsh)  printf "    echo 'export PATH=\"%s:\$PATH\"' >> ~/.zshrc && source ~/.zshrc\n" "$INSTALL_DIR" ;;
                */fish) printf "    fish_add_path %s\n" "$INSTALL_DIR" ;;
                *)      printf "    echo 'export PATH=\"%s:\$PATH\"' >> ~/.bashrc && source ~/.bashrc\n" "$INSTALL_DIR" ;;
            esac
            printf "\n"
            ;;
    esac

    printf "\n${GREEN}${BOLD}sync-devices has been installed successfully!${RESET}\n\n"
}

main
