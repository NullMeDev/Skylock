# Skylock Automated Scheduling Guide

Complete guide for setting up automated backups with systemd timers and desktop notifications.

## Table of Contents

- [Quick Start](#quick-start)
- [Systemd Timer Setup](#systemd-timer-setup)
- [Desktop Notifications](#desktop-notifications)
- [Scheduling Options](#scheduling-options)
- [Configuration](#configuration)
- [Monitoring](#monitoring)
- [Troubleshooting](#troubleshooting)

## Quick Start

### Install Automated Backups (Linux/Ubuntu)

```bash
# 1. Build and install Skylock
cd /path/to/skylock-hybrid
cargo build --release
sudo cp target/release/skylock /usr/local/bin/

# Or install to user directory
mkdir -p ~/.local/bin
cp target/release/skylock ~/.local/bin/
```

### 2. Install Systemd Timer

```bash
./scripts/install-timer.sh
```

This will:
- Install systemd user service and timer
- Enable automatic daily backups at 2:00 AM
- Start the timer immediately

### 3. Verify Installation

```bash
# Check timer status
systemctl --user status skylock-backup.timer

# See next scheduled run
systemctl --user list-timers
```

Done! Your backups will now run automatically.

## Systemd Timer Setup

### What's Included

Skylock includes two systemd unit files:

1. **skylock-backup.service** - The backup service itself
2. **skylock-backup.timer** - Scheduler for automated backups

### Default Schedule

By default, backups run:
- **Time**: Daily at 2:00 AM
- **Randomization**: ±10 minutes to avoid thundering herd
- **Missed runs**: Catch up within 15 minutes after boot
- **Suspended system**: Does not wake system (can be changed)

### Installation Locations

User-level systemd units are installed to:
```
~/.config/systemd/user/skylock-backup.service
~/.config/systemd/user/skylock-backup.timer
```

### Resource Limits

The service includes built-in resource limits:
- **CPU**: Limited to 50% of one core
- **Memory**: Maximum 2GB
- **Tasks**: Maximum 20 concurrent tasks

This prevents backups from slowing down your system.

### Security Features

The service runs with security hardening:
- Private temporary directory
- No new privileges allowed
- Protected system directories
- Read-only home (except config/logs)

## Desktop Notifications

Skylock sends desktop notifications for:

### Backup Events

1. **Backup Started**
   - Shown when backup begins
   - Shows number of paths being backed up
   - 5-second timeout

2. **Backup Completed**
   - Shows success message
   - Displays file count, size, and duration
   - 5-second timeout

3. **Backup Failed**
   - Critical urgency notification
   - Shows error message
   - 10-second timeout (longer for errors)

### Restore Events

1. **Restore Started**
   - Shows backup ID being restored
   - 5-second timeout

2. **Restore Completed**
   - Shows file count and duration
   - 5-second timeout

3. **Restore Failed**
   - Critical urgency notification
   - Shows error message
   - 10-second timeout

### Notification System

Notifications use:
- **Linux**: D-Bus notify-rust library
- **Icons**: System default icons (emblem-default for success, dialog-error for failures)
- **Sound**: Uses system notification sound settings

### Customizing Notifications

Edit your desktop environment notification settings to:
- Change notification position
- Adjust timeout durations
- Enable/disable sounds
- Change notification style

## Scheduling Options

### Change Backup Time

Edit the timer file:

```bash
nano ~/.config/systemd/user/skylock-backup.timer
```

Change the `OnCalendar` line:

```ini
# Run at 3:30 AM daily
OnCalendar=*-*-* 03:30:00

# Run twice daily (6 AM and 6 PM)
OnCalendar=*-*-* 06:00:00
OnCalendar=*-*-* 18:00:00

# Run every 4 hours
OnCalendar=*-*-* 00,04,08,12,16,20:00:00

# Run on weekdays only at 8 PM
OnCalendar=Mon-Fri *-*-* 20:00:00

# Run on the 1st of every month
OnCalendar=*-*-01 02:00:00
```

After editing, reload:

```bash
systemctl --user daemon-reload
systemctl --user restart skylock-backup.timer
```

### Common Schedules

#### Hourly Backups

```ini
OnCalendar=hourly
```

#### Every 6 Hours

```ini
OnCalendar=00/6:00:00
```

#### Weekly (Sunday 3 AM)

```ini
OnCalendar=Sun *-*-* 03:00:00
```

#### Monthly (1st day, 2 AM)

```ini
OnCalendar=*-*-01 02:00:00
```

### Wake System for Backups

To wake system from suspend:

```ini
[Timer]
WakeSystem=true
```

**Warning**: This will wake your laptop/desktop from sleep to run backups.

### Customize Backup Paths

Edit the service to specify custom paths:

```bash
nano ~/.config/systemd/user/skylock-backup.service
```

Change the `ExecStart` line:

```ini
# Backup specific paths
ExecStart=%h/.local/bin/skylock backup --direct /home/user/Documents /home/user/Pictures

# Use configured paths from config file
ExecStart=%h/.local/bin/skylock backup --direct
```

## Configuration

### Config File Location

Skylock reads configuration from:
```
~/.config/skylock-hybrid/config.toml
```

### Required Configuration

Ensure your config includes:

```toml
[hetzner]
endpoint = "https://your-username.your-storagebox.de"
username = "your-username"
password = "your-password"
encryption_key = "your-base64-encryption-key"

[backup]
backup_paths = [
    "/home/user/Documents",
    "/home/user/Pictures",
]
retention_days = 30
schedule = "0 2 * * *"  # 2 AM daily
```

### Environment Variables

The service automatically uses:
- `HOME`: User's home directory
- `USER`: Current username
- `PATH`: System PATH

## Monitoring

### Check Timer Status

```bash
# View timer status
systemctl --user status skylock-backup.timer

# View next scheduled runs
systemctl --user list-timers skylock-backup.timer
```

### View Backup Logs

```bash
# View recent logs
journalctl --user -u skylock-backup.service -n 50

# Follow logs in real-time
journalctl --user -u skylock-backup.service -f

# View logs from specific date
journalctl --user -u skylock-backup.service --since "2025-11-06"

# View only errors
journalctl --user -u skylock-backup.service -p err
```

### Check Last Backup

```bash
# List recent backups
skylock list

# View service logs for last run
journalctl --user -u skylock-backup.service -n 100
```

### Email Notifications

To receive email notifications:

1. Install `mailutils`:
   ```bash
   sudo apt install mailutils
   ```

2. Create a notification script:
   ```bash
   #!/bin/bash
   # ~/bin/skylock-notify.sh
   
   if systemctl --user is-failed --quiet skylock-backup.service; then
       echo "Skylock backup failed on $(date)" | mail -s "Backup Failure" your@email.com
   fi
   ```

3. Add to cron:
   ```bash
   crontab -e
   # Add:
   15 2 * * * ~/bin/skylock-notify.sh
   ```

## Troubleshooting

### Timer Not Running

```bash
# Enable and start timer
systemctl --user enable skylock-backup.timer
systemctl --user start skylock-backup.timer

# Check status
systemctl --user status skylock-backup.timer
```

### Service Failing

```bash
# View detailed error logs
journalctl --user -u skylock-backup.service -n 100

# Test backup manually
skylock backup --direct

# Check config
cat ~/.config/skylock-hybrid/config.toml
```

### No Notifications Appearing

Check if notification daemon is running:

```bash
# Check for notification service
ps aux | grep notification

# Test notifications manually
notify-send "Test" "Testing notifications"
```

If `notify-send` works but Skylock notifications don't:

```bash
# Check Skylock logs
journalctl --user -u skylock-backup.service | grep -i notif

# Verify D-Bus is accessible
echo $DBUS_SESSION_BUS_ADDRESS
```

### Backup Paths Not Configured

Error: "No backup paths specified"

**Solution**: Either:
1. Specify paths in command: `skylock backup --direct /path1 /path2`
2. Or configure in `~/.config/skylock-hybrid/config.toml`:
   ```toml
   [backup]
   backup_paths = ["/path1", "/path2"]
   ```

### Permission Denied

Error: "Permission denied" when accessing files

**Solution**:
- Check file permissions: `ls -la /path/to/backup`
- Ensure user has read access
- For system files, may need different approach

### Missed Backup Runs

If system was off during scheduled time:

```bash
# Check if persistent backups are enabled
grep Persistent ~/.config/systemd/user/skylock-backup.timer

# Should show:
Persistent=true
```

This ensures missed backups run after boot.

### Systemd User Instance Not Running

```bash
# Enable lingering (keeps user services running)
loginctl enable-linger $USER

# Check status
loginctl show-user $USER | grep Linger
```

## Advanced Configuration

### Multiple Backup Profiles

Create multiple timer/service pairs:

```bash
# Copy and rename
cp ~/.config/systemd/user/skylock-backup.service ~/.config/systemd/user/skylock-backup-docs.service
cp ~/.config/systemd/user/skylock-backup.timer ~/.config/systemd/user/skylock-backup-docs.timer

# Edit to backup different paths
nano ~/.config/systemd/user/skylock-backup-docs.service
# Change ExecStart to: skylock backup --direct /home/user/Documents
```

Enable both:
```bash
systemctl --user enable --now skylock-backup-docs.timer
```

### Pre/Post Backup Scripts

Add to service file:

```ini
[Service]
ExecStartPre=/home/user/bin/pre-backup.sh
ExecStart=/home/user/.local/bin/skylock backup --direct
ExecStartPost=/home/user/bin/post-backup.sh
```

Example pre-backup script:
```bash
#!/bin/bash
# Dump database before backup
pg_dump mydb > /home/user/backup/db.sql
```

Example post-backup script:
```bash
#!/bin/bash
# Send success notification
notify-send "Backup Complete" "$(date)"
```

### Backup on Network Change

Create path unit to trigger on network:

```ini
# ~/.config/systemd/user/network-backup.path
[Unit]
Description=Trigger backup on network change

[Path]
PathChanged=/sys/class/net

[Install]
WantedBy=default.target
```

## Best Practices

### Scheduling

1. **Off-peak hours**: Schedule during low-activity times (night/early morning)
2. **Not too frequent**: Daily or weekly is usually sufficient
3. **Stagger schedules**: If multiple backups, offset their times
4. **Consider bandwidth**: Large backups may need dedicated time windows

### Monitoring

1. **Check logs weekly**: Review backup logs regularly
2. **Test restores**: Periodically test restore functionality
3. **Monitor disk space**: Ensure both local and remote have space
4. **Set up alerts**: Configure failure notifications

### Security

1. **Rotate encryption keys**: Periodically update encryption keys
2. **Secure config**: Protect config file permissions
   ```bash
   chmod 600 ~/.config/skylock-hybrid/config.toml
   ```
3. **Review logs**: Check for security-related errors

### Maintenance

1. **Update regularly**: Keep Skylock updated
2. **Clean old backups**: Manually delete very old backups if needed
3. **Test recovery**: Verify you can restore from backups
4. **Document changes**: Keep notes on configuration changes

## Command Reference

```bash
# Timer Management
systemctl --user status skylock-backup.timer     # View status
systemctl --user start skylock-backup.timer      # Start timer
systemctl --user stop skylock-backup.timer       # Stop timer
systemctl --user restart skylock-backup.timer    # Restart timer
systemctl --user enable skylock-backup.timer     # Enable on boot
systemctl --user disable skylock-backup.timer    # Disable on boot

# Service Management
systemctl --user status skylock-backup.service   # View status
systemctl --user start skylock-backup.service    # Run backup now
systemctl --user stop skylock-backup.service     # Stop running backup
systemctl --user restart skylock-backup.service  # Restart backup

# Logs
journalctl --user -u skylock-backup.service      # View all logs
journalctl --user -u skylock-backup.service -f   # Follow logs
journalctl --user -u skylock-backup.service -n 50  # Last 50 lines

# Manual Backups
skylock backup --direct                          # Use config paths
skylock backup --direct /path1 /path2           # Specify paths
skylock list                                     # List backups
```

## Backup Retention

Skylock includes automated backup retention policies to prevent unlimited storage growth.

### Cleanup Command

```bash
# Dry run - see what would be deleted
skylock cleanup --dry-run

# Delete old backups (with confirmation)
skylock cleanup

# Delete without confirmation
skylock cleanup --force
```

### Retention Policy

Default retention policy:
- **Keep Last**: 30 most recent backups
- **Keep Days**: Backups from last 90 days (from config)
- **Minimum Keep**: Always keep at least 3 backups

### How It Works

1. **List Backups**: Fetches all backup manifests
2. **Apply Rules**: Calculates which backups to keep/delete
3. **Safety Check**: Never deletes below minimum threshold
4. **Confirm**: Asks for confirmation (unless --force)
5. **Delete**: Removes old backup files from storage

### Advanced: GFS Rotation

Grandfather-Father-Son rotation can be configured:

```toml
[backup.retention.gfs]
keep_hourly = 24    # Keep hourly for 24 hours
keep_daily = 7      # Keep daily for 7 days
keep_weekly = 4     # Keep weekly for 4 weeks  
keep_monthly = 12   # Keep monthly for 12 months
keep_yearly = 5     # Keep yearly for 5 years
```

### Automated Cleanup

Add cleanup to your systemd service:

```ini
# Run cleanup after backup
ExecStartPost=/home/user/.local/bin/skylock cleanup --force
```

Or create a separate timer:

```ini
# ~/.config/systemd/user/skylock-cleanup.timer
[Unit]
Description=Skylock Backup Cleanup Timer

[Timer]
OnCalendar=weekly

[Install]
WantedBy=timers.target
```

## See Also

- [README.md](README.md) - Main documentation
- [USAGE.md](USAGE.md) - General usage guide
- [RESTORE_GUIDE.md](RESTORE_GUIDE.md) - Restore guide
- [SECURITY.md](SECURITY.md) - Security best practices
