#!/usr/bin/env bash
# ScoutChain — initialize all deployed contracts and wire cross-contract links.
# Run after deploy.sh. Requires .env.contracts to exist.
# Usage: ./scripts/initialize.sh [testnet|mainnet]
#
# Requires (in .env or environment):
#   ADMIN_ADDRESS      — Stellar G-address that will own all contracts
#   DEPLOYER_SECRET    — Stellar secret key used to invoke contracts
#   XLM_TOKEN_ADDRESS  — Native XLM token contract address on the target network
#
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

invoke_contract() {
  # invoke_contract <label> <contract_id> [extra stellar args...]
  local label="$1"; shift
  local contract_id="$1"; shift

  echo "==> $label..."
  stellar contract invoke \
    --id "$contract_id" \
    --source "$DEPLOYER" \
    --network "$NETWORK" \
    "$@" \
    || die "$label failed."
  echo "    OK"
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

ADMIN="${ADMIN_ADDRESS:?Set ADMIN_ADDRESS in .env or environment}"
DEPLOYER="${DEPLOYER_SECRET:?Set DEPLOYER_SECRET in .env or environment}"
XLM_TOKEN="${XLM_TOKEN_ADDRESS:?Set XLM_TOKEN_ADDRESS in .env or environment}"

echo "==> Initializing contracts on $NETWORK"
echo "    Admin:        $ADMIN"
echo "    Registration: $REGISTRATION_CONTRACT_ID"
echo "    Verification: $VERIFICATION_CONTRACT_ID"
echo "    Progress:     $PROGRESS_CONTRACT_ID"
echo "    Scout Access: $SCOUT_ACCESS_CONTRACT_ID"
echo ""

# ---------------------------------------------------------------------------
# Initialize each contract
# ---------------------------------------------------------------------------

invoke_contract \
  "Initializing registration contract" \
  "$REGISTRATION_CONTRACT_ID" \
  -- initialize \
  --admin "$ADMIN"

invoke_contract \
  "Initializing verification contract" \
  "$VERIFICATION_CONTRACT_ID" \
  -- initialize \
  --admin "$ADMIN"

invoke_contract \
  "Initializing progress contract" \
  "$PROGRESS_CONTRACT_ID" \
  -- initialize \
  --admin "$ADMIN"

invoke_contract \
  "Initializing scout_access contract" \
  "$SCOUT_ACCESS_CONTRACT_ID" \
  -- initialize \
  --admin "$ADMIN" \
  --xlm_token "$XLM_TOKEN" \
  --fee_config '{
    "contact_fee_stroops": 1000000,
    "basic_sub_stroops": 10000000,
    "pro_sub_stroops": 30000000,
    "elite_sub_stroops": 70000000,
    "sub_duration_secs": 2592000
  }'

# ---------------------------------------------------------------------------
# Wire cross-contract links
# ---------------------------------------------------------------------------

invoke_contract \
  "Wiring verification → progress (set_progress_contract)" \
  "$VERIFICATION_CONTRACT_ID" \
  -- set_progress_contract \
  --progress_contract "$PROGRESS_CONTRACT_ID"

invoke_contract \
  "Wiring scout_access → progress (set_progress_contract)" \
  "$SCOUT_ACCESS_CONTRACT_ID" \
  -- set_progress_contract \
  --addr "$PROGRESS_CONTRACT_ID"

invoke_contract \
  "Wiring registration → progress (set_progress_contract)" \
  "$REGISTRATION_CONTRACT_ID" \
  -- set_progress_contract \
  --addr "$PROGRESS_CONTRACT_ID"

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------

echo ""
echo "==> All contracts initialized and wired successfully."
echo "    Run ./scripts/generate-bindings.sh $NETWORK to produce TypeScript clients."
