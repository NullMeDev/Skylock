#!/bin/bash
# Install Skylock systemd timer for automated backups

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SKYLOCK_SYSTEMD_DIR="$SCRIPT_DIR/../systemd"

echo "🔧 Installing Skylock systemd timer..."
echo

# Create systemd user directory if it doesn't exist
mkdir -p "$SYSTEMD_USER_DIR"

# Copy service and timer files
echo "📋 Copying systemd unit files..."
cp "$SKYLOCK_SYSTEMD_DIR/skylock-backup.service" "$SYSTEMD_USER_DIR/"
cp "$SKYLOCK_SYSTEMD_DIR/skylock-backup.timer" "$SYSTEMD_USER_DIR/"

# Reload systemd daemon
echo "🔄 Reloading systemd daemon..."
systemctl --user daemon-reload

# Enable and start the timer
echo "▶️  Enabling and starting timer..."
systemctl --user enable skylock-backup.timer
systemctl --user start skylock-backup.timer

echo
echo "✅ Skylock timer installed successfully!"
echo
echo "📊 Timer status:"
systemctl --user status skylock-backup.timer --no-pager
echo
echo "📅 Next scheduled run:"
systemctl --user list-timers skylock-backup.timer --no-pager
echo
echo "💡 Useful commands:"
echo "   View timer status:    systemctl --user status skylock-backup.timer"
echo "   View service logs:    journalctl --user -u skylock-backup.service"
echo "   Stop timer:           systemctl --user stop skylock-backup.timer"
echo "   Disable timer:        systemctl --user disable skylock-backup.timer"
echo "   Trigger backup now:   systemctl --user start skylock-backup.service"
echo
