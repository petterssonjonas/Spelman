#!/usr/bin/env bash
set -euo pipefail

# Spelman installer — downloads prebuilt binary or builds from source.
# Usage: curl -fsSL https://raw.githubusercontent.com/petterssonjonas/Spelman/main/install.sh | bash

REPO="petterssonjonas/Spelman"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

info()  { printf '\033[1;34m::\033[0m %s\n' "$*"; }
error() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *) error "Unsupported architecture: $(uname -m)" ;;
    esac
}

install_prebuilt() {
    local arch="$1"
    local version

    info "Fetching latest release..."
    version=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')

    if [ -z "$version" ]; then
        return 1
    fi

    local tarball="spelman-${version}-${arch}-linux.tar.gz"
    local url="https://github.com/${REPO}/releases/download/v${version}/${tarball}"

    info "Downloading spelman v${version} for ${arch}..."
    local tmp
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    if curl -fsSL "$url" -o "$tmp/$tarball"; then
        tar xzf "$tmp/$tarball" -C "$tmp"
        mkdir -p "$INSTALL_DIR"
        install -m755 "$tmp/spelman" "$INSTALL_DIR/spelman"
        return 0
    fi

    return 1
}

build_from_source() {
    info "No prebuilt binary available — building from source..."

    if ! command -v cargo &>/dev/null; then
        error "Rust toolchain not found. Install it: https://rustup.rs"
    fi

    # Check for ALSA headers
    if [ "$(uname)" = "Linux" ]; then
        if ! pkg-config --exists alsa 2>/dev/null; then
            error "ALSA development headers not found.\n  Fedora: sudo dnf install alsa-lib-devel\n  Debian: sudo apt install libasound2-dev"
        fi
    fi

    local tmp
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT

    info "Cloning repository..."
    git clone --depth 1 "https://github.com/${REPO}.git" "$tmp/spelman"

    info "Building (release mode)..."
    cargo build --release --manifest-path "$tmp/spelman/Cargo.toml"

    mkdir -p "$INSTALL_DIR"
    install -m755 "$tmp/spelman/target/release/spelman" "$INSTALL_DIR/spelman"
}

main() {
    info "Installing spelman to ${INSTALL_DIR}"

    local arch
    arch=$(detect_arch)

    if ! install_prebuilt "$arch"; then
        build_from_source
    fi

    info "Installed spelman to ${INSTALL_DIR}/spelman"

    # Check if INSTALL_DIR is in PATH
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            printf '\n\033[1;33mNote:\033[0m %s is not in your PATH.\n' "$INSTALL_DIR"
            printf 'Add it with: export PATH="%s:$PATH"\n' "$INSTALL_DIR"
            ;;
    esac
}

main "$@"
