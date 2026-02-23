#!/usr/bin/env bash
# gRPC debugging script for Hayate UTxORPC server
#
# Usage: ./scripts/debug-grpc.sh <command> [args...]
#
# Requirements:
#   - grpcurl (https://github.com/fullstorydev/grpcurl)
#   - hayate server running on localhost:50051 with gRPC reflection enabled

set -euo pipefail

ENDPOINT="${UTXORPC_ENDPOINT:-localhost:50051}"
PLAINTEXT="${UTXORPC_PLAINTEXT:--plaintext}"

# CNight token policy ID (the large ~8.2MB dataset that was causing issues)
CNIGHT_POLICY="d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1"

case "${1:-help}" in
    list-services)
        echo "=== Listing all gRPC services ==="
        grpcurl $PLAINTEXT "$ENDPOINT" list
        ;;

    list-methods)
        echo "=== Listing QueryService methods ==="
        grpcurl $PLAINTEXT "$ENDPOINT" list utxorpc.query.v1.QueryService
        ;;

    describe)
        method="${2:-utxorpc.query.v1.QueryService.ReadUtxos}"
        echo "=== Describing $method ==="
        grpcurl $PLAINTEXT "$ENDPOINT" describe "$method"
        ;;

    get-chain-tip)
        echo "=== Getting chain tip ==="
        grpcurl $PLAINTEXT -d '{}' "$ENDPOINT" utxorpc.query.v1.QueryService.GetChainTip
        ;;

    read-params)
        echo "=== Reading chain parameters ==="
        grpcurl $PLAINTEXT -d '{}' "$ENDPOINT" utxorpc.query.v1.QueryService.ReadParams
        ;;

    read-utxos)
        if [ $# -lt 2 ]; then
            echo "Usage: $0 read-utxos <address_hex>"
            echo "Example: $0 read-utxos 70fc4b0c3aaa6d7d1d0f4f1c6e18e8f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7"
            exit 1
        fi
        address_hex="$2"
        echo "=== Reading UTxOs for address: $address_hex ==="
        grpcurl $PLAINTEXT -d "{\"addresses\": [\"$address_hex\"]}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.ReadUtxos
        ;;

    search-utxos)
        pattern="${2:-*}"
        echo "=== Searching UTxOs with pattern: $pattern ==="
        grpcurl $PLAINTEXT -d "{\"pattern\": \"$pattern\"}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.SearchUtxos
        ;;

    get-tx-history)
        if [ $# -lt 2 ]; then
            echo "Usage: $0 get-tx-history <address_hex> [max_txs]"
            echo "Example: $0 get-tx-history 70fc4b0c3aaa6d7d1d0f4f1c6e18e8f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7 10"
            exit 1
        fi
        address_hex="$2"
        max_txs="${3:-100}"
        echo "=== Getting transaction history for address: $address_hex (max: $max_txs) ==="
        grpcurl $PLAINTEXT -d "{\"address\": \"$address_hex\", \"max_txs\": $max_txs}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.GetTxHistory
        ;;

    read-utxo-events)
        start_slot="${2:-0}"
        end_slot="${3:-1000}"
        max_events="${4:-100}"
        echo "=== Reading UTxO events: slots $start_slot-$end_slot (max: $max_events) ==="
        grpcurl $PLAINTEXT -d "{\"start_slot\": $start_slot, \"end_slot\": $end_slot, \"max_events\": $max_events}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.ReadUtxoEvents
        ;;

    get-block)
        if [ $# -lt 2 ]; then
            echo "Usage: $0 get-block <block_hash_hex>"
            echo "Example: $0 get-block 5f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7"
            exit 1
        fi
        block_hash_hex="$2"
        echo "=== Getting block by hash: $block_hash_hex ==="
        grpcurl $PLAINTEXT -d "{\"hash\": \"$block_hash_hex\"}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.GetBlockByHash
        ;;

    test-cnight)
        echo "=== Testing CNight token query (the ~8.2MB dataset) ==="
        echo "This query was causing the 'decoded message length too large' error"
        echo "Policy ID: $CNIGHT_POLICY"
        echo ""
        # Note: This searches for UTxOs by policy ID via address pattern matching
        # The actual midnight-node query may be more complex
        echo "Attempting query..."
        grpcurl $PLAINTEXT -max-msg-sz 134217728 -d "{\"pattern\": \"$CNIGHT_POLICY\"}" \
            "$ENDPOINT" utxorpc.query.v1.QueryService.SearchUtxos || true
        ;;

    help|*)
        cat <<EOF
Hayate UTxORPC gRPC Debugging Script

Usage: $0 <command> [args...]

Commands:
  list-services              List all gRPC services
  list-methods               List QueryService methods
  describe [method]          Describe a gRPC method (default: ReadUtxos)
  get-chain-tip              Get current chain tip
  read-params                Read chain parameters
  read-utxos <addr>          Read UTxOs for an address (hex)
  search-utxos [pattern]     Search UTxOs by pattern (default: *)
  get-tx-history <addr> [n]  Get transaction history for address (max n txs)
  read-utxo-events <start> <end> [max]  Read UTxO events in slot range
  get-block <hash>           Get block by hash (hex)
  test-cnight                Test the large CNight token dataset query
  help                       Show this help message

Environment Variables:
  UTXORPC_ENDPOINT          gRPC endpoint (default: localhost:50051)
  UTXORPC_PLAINTEXT         Use plaintext connection (default: -plaintext)

Examples:
  # List all available services
  $0 list-services

  # Get current chain tip
  $0 get-chain-tip

  # Read UTxOs for an address
  $0 read-utxos 70fc4b0c3aaa6d7d1d0f4f1c6e18e8f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7

  # Test the large CNight dataset
  $0 test-cnight

Note: Requires grpcurl to be installed and hayate server running with gRPC reflection.
EOF
        ;;
esac
