#!/bin/bash

# Skylock Installation Script
# Makes Skylock easy to use system-wide

set -e

echo "🚀 Installing Skylock..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Check if we're in the right directory
if [[ ! -f "Cargo.toml" ]] || [[ ! -d "target/debug" ]]; then
    echo -e "${RED}❌ Error: Please run this script from the skylock-hybrid project directory${NC}"
    exit 1
fi

# Create local bin directory if it doesn't exist
BIN_DIR="$HOME/.local/bin"
mkdir -p "$BIN_DIR"

# Build the project if binary doesn't exist or is outdated
if [[ ! -f "target/debug/skylock" ]] || [[ "Cargo.toml" -nt "target/debug/skylock" ]]; then
    echo -e "${BLUE}📦 Building Skylock...${NC}"
    cargo build --bin skylock
    echo -e "${GREEN}✅ Build complete${NC}"
fi

# Copy binary to local bin
echo -e "${BLUE}📋 Installing binary...${NC}"
cp target/debug/skylock "$BIN_DIR/skylock"
chmod +x "$BIN_DIR/skylock"
echo -e "${GREEN}✅ Binary installed to $BIN_DIR/skylock${NC}"

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo -e "${YELLOW}⚠️  Adding ~/.local/bin to PATH${NC}"
    
    # Determine shell config file
    SHELL_CONFIG=""
    if [[ -n "$ZSH_VERSION" ]]; then
        SHELL_CONFIG="$HOME/.zshrc"
    elif [[ -n "$BASH_VERSION" ]]; then
        SHELL_CONFIG="$HOME/.bashrc"
    else
        SHELL_CONFIG="$HOME/.profile"
    fi
    
    # Add to PATH if not already there
    if ! grep -q 'export PATH="$HOME/.local/bin:$PATH"' "$SHELL_CONFIG" 2>/dev/null; then
        echo '' >> "$SHELL_CONFIG"
        echo '# Added by Skylock installer' >> "$SHELL_CONFIG"
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$SHELL_CONFIG"
        echo -e "${GREEN}✅ Added to PATH in $SHELL_CONFIG${NC}"
        echo -e "${YELLOW}📝 Please run: source $SHELL_CONFIG${NC}"
        echo -e "${YELLOW}   Or restart your terminal${NC}"
    fi
else
    echo -e "${GREEN}✅ ~/.local/bin already in PATH${NC}"
fi

# Create config directory
CONFIG_DIR="$HOME/.config/skylock-hybrid"
mkdir -p "$CONFIG_DIR"
echo -e "${GREEN}✅ Config directory created: $CONFIG_DIR${NC}"

# Generate default config if it doesn't exist
if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
    echo -e "${BLUE}📄 Generating default configuration...${NC}"
    "$BIN_DIR/skylock" config > /dev/null 2>&1 || true
    if [[ -f "$CONFIG_DIR/config.toml" ]]; then
        echo -e "${GREEN}✅ Default config created${NC}"
    fi
fi

# Create desktop entry for GUI users
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/skylock.desktop" << EOF
[Desktop Entry]
Name=Skylock
Comment=Secure backup tool for Hetzner Storage Box
Exec=$BIN_DIR/skylock
Icon=drive-harddisk
Terminal=true
Type=Application
Categories=System;Utility;Archiving;
Keywords=backup;hetzner;encryption;storage;
StartupNotify=true
EOF
echo -e "${GREEN}✅ Desktop entry created${NC}"

# Create a convenient wrapper script for common operations
cat > "$BIN_DIR/skylock-setup" << 'EOF'
#!/bin/bash
# Skylock Setup Helper

echo "🔧 Skylock Quick Setup"
echo ""
echo "1. Configure credentials"
echo "2. Test system"
echo "3. Create first backup"
echo "4. List backups"
echo "5. Help"
echo ""
read -p "Choose option (1-5): " choice

case $choice in
    1)
        echo "Opening config file for editing..."
        ${EDITOR:-nano} ~/.config/skylock-hybrid/config.toml
        ;;
    2)
        skylock test all
        ;;
    3)
        echo "Enter directory to backup:"
        read -r backup_dir
        if [[ -d "$backup_dir" ]]; then
            skylock backup "$backup_dir"
        else
            echo "Directory does not exist: $backup_dir"
        fi
        ;;
    4)
        skylock list --detailed
        ;;
    5)
        skylock --help
        ;;
    *)
        echo "Invalid option"
        ;;
esac
EOF
chmod +x "$BIN_DIR/skylock-setup"
echo -e "${GREEN}✅ Setup helper created: skylock-setup${NC}"

# Test installation
echo -e "${BLUE}🧪 Testing installation...${NC}"
if "$BIN_DIR/skylock" --version > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Installation successful!${NC}"
else
    echo -e "${RED}❌ Installation test failed${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}🎉 Skylock Installation Complete!${NC}"
echo ""
echo -e "${BLUE}📋 Quick Start:${NC}"
echo -e "   1. Restart your terminal or run: ${YELLOW}source ~/.zshrc${NC}"
echo -e "   2. Run: ${YELLOW}skylock config${NC} (to setup)"
echo -e "   3. Edit: ${YELLOW}~/.config/skylock-hybrid/config.toml${NC}"
echo -e "   4. Test: ${YELLOW}skylock test all${NC}"
echo -e "   5. Backup: ${YELLOW}skylock backup /path/to/backup${NC}"
echo ""
echo -e "${BLUE}🔧 Convenience commands:${NC}"
echo -e "   • ${YELLOW}skylock-setup${NC} - Interactive setup helper"
echo -e "   • ${YELLOW}skylock --help${NC} - Full help"
echo ""
echo -e "${GREEN}Ready to use! 🚀${NC}"