#!/usr/bin/env bash
# Run Hayate live integration tests
#
# This script runs integration tests against a live Hayate node.
# The node must be running and synced at localhost:50051 (or HAYATE_API env var)
#
# Usage:
#   ./run-live-tests.sh                    # Run all live tests
#   ./run-live-tests.sh test_name          # Run specific test
#   HAYATE_API=http://localhost:50053 ./run-live-tests.sh  # Custom endpoint

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default endpoint
HAYATE_API="${HAYATE_API:-http://127.0.0.1:50051}"

echo -e "${YELLOW}Hayate Live Integration Tests${NC}"
echo "================================"
echo ""
echo "Endpoint: $HAYATE_API"
echo ""

# Check if node is accessible
if ! grpcurl -plaintext "${HAYATE_API#http://}" list > /dev/null 2>&1; then
    echo -e "${RED}WARNING: Cannot connect to Hayate node at $HAYATE_API${NC}"
    echo "Make sure your Hayate node is running and synced."
    echo ""
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Run tests
if [ $# -eq 0 ]; then
    echo "Running all live integration tests..."
    echo ""
    cargo test --test live_integration_tests -- --ignored --nocapture
else
    echo "Running test: $1"
    echo ""
    cargo test --test live_integration_tests "$1" -- --ignored --nocapture
fi

exit_code=$?

if [ $exit_code -eq 0 ]; then
    echo ""
    echo -e "${GREEN}✓ All tests passed!${NC}"
else
    echo ""
    echo -e "${RED}✗ Some tests failed${NC}"
fi

exit $exit_code
