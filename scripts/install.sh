#!/usr/bin/env bash
set -euo pipefail

# termchat installer — downloads the latest release binary
# Usage: curl -sSfL https://raw.githubusercontent.com/LHagfoss/termchat/main/scripts/install.sh | bash
# Or clone and run: ./scripts/install.sh

REPO="LHagfoss/termchat"
INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="termchat"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[✓]${NC} $*"; }
warn()  { echo -e "${YELLOW}[!]${NC} $*"; }
error() { echo -e "${RED}[✗]${NC} $*" >&2; exit 1; }

detect_os() {
  case "$(uname -s | tr '[:upper:]' '[:lower:]')" in
    linux*)     echo "linux" ;;
    darwin*)    echo "darwin" ;;
    msys*|cygwin*|mingw*) echo "windows" ;;
    *)          error "Unsupported OS: $(uname -s)" ;;
  esac
}

detect_arch() {
  local arch
  arch="$(uname -m)"
  case "$arch" in
    x86_64)   echo "x86_64" ;;
    aarch64|arm64) echo "aarch64" ;;
    armv7l)   error "ARM 32-bit is not supported. Use a 64-bit system." ;;
    *)        error "Unsupported arch: $arch" ;;
  esac
}

download_asset() {
  local os arch name ext
  os="$1"; arch="$2"; name="$3"; ext="$4"
  
  # Try GitHub API for the latest release
  local api_url="https://api.github.com/repos/$REPO/releases/latest"
  local asset_url
  asset_url=$(curl -sL "$api_url" \
    | jq -r ".assets[] | select(.name == \"${name}.${ext}\") | .browser_download_url" \
    2>/dev/null)

  if [[ -z "$asset_url" ]]; then
    error "No release asset found: ${name}.${ext}. Check https://github.com/$REPO/releases"
  fi

  echo "Downloading $asset_url ..."
  curl -sSfL --proto '=https' --tlsv1.2 -o "/tmp/${BINARY_NAME}_install.${ext}" "$asset_url"
}

main() {
  local os arch asset_name asset_ext
  
  os="$(detect_os)"
  arch="$(detect_arch)"
  
  echo "Detecting system: $os / $arch"

  case "$os" in
    linux|darwin)
      asset_name="termchat-${arch}-${os}"
      asset_ext="tar.gz"
      download_asset "$os" "$arch" "$asset_name" "$asset_ext"
      
      mkdir -p "$INSTALL_DIR"
      tar xzf "/tmp/${BINARY_NAME}_install.$asset_ext" -C "/tmp/${BINARY_NAME}_install/"
      mv "/tmp/${BINARY_NAME}_install/$asset_name" "$INSTALL_DIR/$BINARY_NAME"
      chmod +x "$INSTALL_DIR/$BINARY_NAME"
      
      info "Installed $BINARY_NAME to $INSTALL_DIR/$BINARY_NAME"
      
      # Check if ~/.local/bin is in PATH
      if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "$INSTALL_DIR is not in your PATH."
        echo "Add this to your shell config:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
      fi
      ;;
      
    windows)
      asset_name="termchat-${arch}-windows"
      asset_ext="exe"
      download_asset "$os" "$arch" "$asset_name" "$asset_ext"
      
      mv "/tmp/${BINARY_NAME}_install.$asset_ext" "$INSTALL_DIR/$BINARY_NAME.exe"
      
      info "Installed $BINARY_NAME.exe to $INSTALL_DIR/$BINARY_NAME.exe"
      warn "To run in PowerShell, you may need to bypass execution policy:"
      echo "  Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy Bypass"
      ;;
  esac

  # Cleanup
  rm -rf "/tmp/${BINARY_NAME}_install" "/tmp/${BINARY_NAME}_install.*"
  
  info "Run '$BINARY_NAME --help' to get started!"
}

main "$@"
