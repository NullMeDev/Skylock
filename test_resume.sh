#!/bin/bash
# Test script for resume functionality
#
# This script demonstrates the resume capability by:
# 1. Creating test files
# 2. Starting a backup
# 3. Simulating interruption (you'll manually Ctrl+C)
# 4. Resuming the backup

set -e

echo "🧪 Testing Resume Functionality"
echo "=============================="
echo

# Create test directory
TEST_DIR="/tmp/skylock_resume_test"
mkdir -p "$TEST_DIR"

# Create test files
echo "📁 Creating test files in $TEST_DIR..."
for i in {1..10}; do
    dd if=/dev/urandom of="$TEST_DIR/file_$i.dat" bs=1M count=5 2>/dev/null
    echo "   Created file_$i.dat (5MB)"
done
echo

echo "✅ Test setup complete!"
echo
echo "📋 Instructions:"
echo "1. Run: cargo run --release --bin skylock -- backup --direct $TEST_DIR"
echo "2. Wait for 2-3 files to upload"
echo "3. Press Ctrl+C to interrupt"
echo "4. Run the same command again - it will resume automatically!"
echo
echo "Expected behavior:"
echo "- First run: Uploads start from beginning"
echo "- After Ctrl+C: Backup interrupted, state saved"
echo "- Second run: Shows 'Resuming interrupted backup' message"
echo "- Second run: Skips already-uploaded files"
echo "- Second run: Continues from where it left off"
echo
echo "State files are stored in:"
echo "  ~/.local/share/skylock-hybrid/resume_state/"
echo
echo "To clean up after testing:"
echo "  rm -rf $TEST_DIR"
echo "  rm -rf ~/.local/share/skylock-hybrid/resume_state/"
echo
