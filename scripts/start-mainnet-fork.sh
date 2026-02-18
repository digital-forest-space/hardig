#!/usr/bin/env bash
# Start a local validator with mainnet Mayflower accounts cloned.
# Usage: ./scripts/start-mainnet-fork.sh [--reset]
#
# Requires: solana-test-validator

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BPF_SO="$PROJECT_ROOT/target/deploy/hardig.so"
PROGRAM_ID="4U2Pgjdq51NXUEDVX4yyFNMdg6PuLHs9ikn9JThkn21p"
LEDGER_DIR="$PROJECT_ROOT/.test-ledger"

if [ ! -f "$BPF_SO" ]; then
    echo "Error: Build with 'anchor build' first ($BPF_SO not found)"
    exit 1
fi

RESET_FLAG=""
if [[ "${1:-}" == "--reset" ]]; then
    RESET_FLAG="--reset"
    rm -rf "$LEDGER_DIR"
fi

# Mayflower program + all navSOL market accounts to clone from mainnet.
exec solana-test-validator \
    --ledger "$LEDGER_DIR" \
    $RESET_FLAG \
    --url mainnet-beta \
    --bpf-program "$PROGRAM_ID" "$BPF_SO" \
    --clone-upgradeable-program AVMmmRzwc2kETQNhPiFVnyu62HrgsQXTD6D7SnSfEz7v \
    --clone 81JEJdJSZbaXixpD8WQSBWBfkDa6m6KpXpSErzYUHq6z \
    --clone Lmdgb4NE4T3ubmQZQZQZ7t4UP6A98NdVbmZPcoEdkdC \
    --clone DotD4dZAyr4Kb6AD3RHid8VgmsHUzWF6LRd4WvAMezRj \
    --clone A5M1nWfi6ATSamEJ1ASr2FC87BMwijthTbNRYG7BhYSc \
    --clone 43vPhZeow3pgYa6zrPXASVQhdXTMfowyfNK87BYizhnL \
    --clone BCYzijbWwmqRnsTWjGhHbneST2emQY36WcRAkbkhsQMt \
    --clone B8jccpiKZjapgfw1ay6EH3pPnxqTmimsm2KsTZ9LSmjf \
    --clone EKVkmuwDKRKHw85NPTbKSKuS75EY4NLcxe1qzSPixLdy \
    --clone navSnrYJkCxMiyhM3F7K889X1u8JFLVHHLxiyo6Jjqo \
    --clone TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA \
    --clone-upgradeable-program metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s
