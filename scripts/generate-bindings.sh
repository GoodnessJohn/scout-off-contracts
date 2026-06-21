#!/usr/bin/env bash
# ScoutChain — generate TypeScript bindings for all deployed contracts.
# Usage: ./scripts/generate-bindings.sh [testnet|mainnet]
#
# Requires .env.contracts to exist (written by deploy.sh).
# Exits non-zero immediately if any step fails (set -euo pipefail).
set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "ERROR: $*" >&2; exit 1; }

require_nonempty() {
  local value="$1"
  local label="$2"
  [[ -n "$value" ]] || die "$label is empty. Re-run deploy.sh and ensure it succeeded."
}

# ---------------------------------------------------------------------------
# Pre-flight: validate .env.contracts exists and all IDs are non-empty
# ---------------------------------------------------------------------------

NETWORK="${1:-testnet}"

[[ -f .env.contracts ]] || die ".env.contracts not found. Run ./scripts/deploy.sh $NETWORK first."

# shellcheck source=/dev/null
source .env.contracts

require_nonempty "${REGISTRATION_CONTRACT_ID:-}"  "REGISTRATION_CONTRACT_ID"
require_nonempty "${VERIFICATION_CONTRACT_ID:-}"  "VERIFICATION_CONTRACT_ID"
require_nonempty "${PROGRESS_CONTRACT_ID:-}"      "PROGRESS_CONTRACT_ID"
require_nonempty "${SCOUT_ACCESS_CONTRACT_ID:-}"  "SCOUT_ACCESS_CONTRACT_ID"

# ---------------------------------------------------------------------------
# Generate bindings for each contract
# ---------------------------------------------------------------------------

CONTRACTS=(registration verification progress scout_access)

declare -A IDS=(
  [registration]="$REGISTRATION_CONTRACT_ID"
  [verification]="$VERIFICATION_CONTRACT_ID"
  [progress]="$PROGRESS_CONTRACT_ID"
  [scout_access]="$SCOUT_ACCESS_CONTRACT_ID"
)

for name in "${CONTRACTS[@]}"; do
  id="${IDS[$name]}"
  out="bindings/${name}"

  echo "==> Generating TypeScript bindings for $name ($id)..."
  stellar contract bindings typescript \
    --contract-id "$id" \
    --network "$NETWORK" \
    --output-dir "$out" \
    --overwrite \
    || die "Binding generation failed for $name."

  echo "    Written to $out/"
done

echo ""
echo "==> All bindings generated. Publish or link them into backend/frontend."
