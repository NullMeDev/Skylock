#!/bin/bash

# Skylock Uninstaller Script
# Removes Skylock cleanly from the system

echo "🗑️  Uninstalling Skylock..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

BIN_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/skylock-hybrid"
DESKTOP_DIR="$HOME/.local/share/applications"

# Remove binaries
if [[ -f "$BIN_DIR/skylock" ]]; then
    rm "$BIN_DIR/skylock"
    echo -e "${GREEN}✅ Removed skylock binary${NC}"
fi

if [[ -f "$BIN_DIR/skylock-setup" ]]; then
    rm "$BIN_DIR/skylock-setup"
    echo -e "${GREEN}✅ Removed skylock-setup helper${NC}"
fi

# Remove desktop entry
if [[ -f "$DESKTOP_DIR/skylock.desktop" ]]; then
    rm "$DESKTOP_DIR/skylock.desktop"
    echo -e "${GREEN}✅ Removed desktop entry${NC}"
fi

# Ask about config directory
if [[ -d "$CONFIG_DIR" ]]; then
    echo -e "${YELLOW}⚠️  Configuration directory exists: $CONFIG_DIR${NC}"
    echo "This contains your Skylock configuration and may contain backup metadata."
    read -p "Remove configuration directory? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$CONFIG_DIR"
        echo -e "${GREEN}✅ Removed configuration directory${NC}"
    else
        echo -e "${BLUE}ℹ️  Kept configuration directory${NC}"
    fi
fi

# Note about PATH
echo -e "${BLUE}ℹ️  Note: PATH modifications in your shell config were not removed${NC}"
echo -e "   You can manually remove this line from ~/.zshrc or ~/.bashrc:"
echo -e "   ${YELLOW}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"

echo ""
echo -e "${GREEN}🎉 Skylock uninstalled successfully!${NC}"