#!/bin/bash

# Get credentials from config
CONFIG_FILE="$HOME/.config/skylock-hybrid/config.toml"
USERNAME=$(grep "username" "$CONFIG_FILE" | cut -d'"' -f2)
PASSWORD=$(grep "password" "$CONFIG_FILE" | cut -d'"' -f2)
ENDPOINT=$(grep "endpoint" "$CONFIG_FILE" | cut -d'"' -f2)

echo "Testing upload to Hetzner Storage Box..."
echo "Endpoint: $ENDPOINT"
echo "Username: $USERNAME"
echo ""

# Upload test file
echo "Uploading test file..."
curl -u "$USERNAME:$PASSWORD" \
  -T /tmp/test_skylock.txt \
  "$ENDPOINT/TEST_SKYLOCK_UPLOAD.txt" \
  -v 2>&1 | grep -E "(< HTTP|> PUT|< Location)"

echo ""
echo "Listing root directory..."
curl -u "$USERNAME:$PASSWORD" \
  -X PROPFIND \
  -H "Depth: 1" \
  "$ENDPOINT/" \
  2>&1 | grep -E "(<D:href>|TEST_SKYLOCK|skylock_backup)"

echo ""
echo "Checking if our test file exists..."
curl -u "$USERNAME:$PASSWORD" \
  -I "$ENDPOINT/TEST_SKYLOCK_UPLOAD.txt" \
  2>&1 | grep -E "(HTTP|Content-Length)"
