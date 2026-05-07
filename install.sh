#!/usr/bin/env bash
# auddyseus install script
# Installs build dependencies, compiles, and places the binary in ~/.local/bin

set -e

echo "==> auddyseus installer"
echo ""

# Check for Rust
if ! command -v cargo &>/dev/null; then
    echo "Rust/cargo not found. Installing via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

RUST_VER=$(rustc --version | awk '{print $2}' | tr -d '.')
echo "Rust version: $(rustc --version)"

# System deps
echo ""
echo "==> Installing system dependencies (requires sudo)..."
sudo apt-get install -y libasound2-dev libssl-dev pkg-config

# Build
echo ""
echo "==> Building auddyseus (this takes a minute)..."
cd "$(dirname "$0")"
cargo build --release

# Install
mkdir -p "$HOME/.local/bin"
cp target/release/auddyseus "$HOME/.local/bin/auddyseus"
chmod +x "$HOME/.local/bin/auddyseus"

echo ""
echo "==> Installed to ~/.local/bin/auddyseus"
echo ""

# PATH check
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo "NOTE: ~/.local/bin is not in your PATH."
    echo "Add this to your ~/.bashrc or ~/.zshrc:"
    echo ""
    echo '    export PATH="$HOME/.local/bin:$PATH"'
    echo ""
fi

echo "==> Config file will be created at:"
echo "    ~/.config/auddyseus/config.toml"
echo "    (on first run — edit it to add your own streams and set music_dir)"
echo ""
echo "Run: auddyseus"
