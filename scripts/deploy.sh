#!/usr/bin/env bash
# ScoutChain — deploy all four contracts to Stellar testnet or mainnet.
# Usage: ./scripts/deploy.sh [testnet|mainnet]
# See docs/DEPLOYMENT.md for full deployment guide.
#
# Requires:
#   DEPLOYER_SECRET — Stellar secret key used to deploy
#
# Writes contract IDs to .env.contracts on success.
# Exits non-zero immediately if any step fails (set -euo pipefail).
set -euo pipefail

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "ERROR: $*" >&2; exit 1; }

require_nonempty() {
  local value="$1"
  local label="$2"
  [[ -n "$value" ]] || die "$label is empty — deploy step likely failed."
}

# ---------------------------------------------------------------------------
# Pre-flight: check required env vars and deployer balance
# ---------------------------------------------------------------------------

NETWORK="${1:-testnet}"
DEPLOYER="${DEPLOYER_SECRET:-}"

[[ -n "$DEPLOYER" ]] || die "Set DEPLOYER_SECRET env var to your Stellar secret key."

# Derive the deployer public address so we can check its balance.
DEPLOYER_ADDRESS=$(stellar keys address --secret-key "$DEPLOYER" 2>/dev/null \
  || die "Could not derive deployer address from DEPLOYER_SECRET.")

echo "==> Pre-flight: checking deployer balance ($DEPLOYER_ADDRESS)..."

# stellar balance returns lines like "XLM: 9999.9999900"
# We need at least ~10 XLM to cover four deploys + fees.
MIN_XLM=10

BALANCE_LINE=$(stellar account balance "$DEPLOYER_ADDRESS" --network "$NETWORK" 2>/dev/null \
  || die "Could not fetch account balance. Is the account funded on $NETWORK?")

# Extract the XLM balance (integer part is sufficient for the threshold check)
XLM_BALANCE=$(echo "$BALANCE_LINE" \
  | grep -Eo '[0-9]+\.[0-9]+' \
  | head -1 \
  | cut -d. -f1)

if [[ -z "$XLM_BALANCE" ]]; then
  die "Could not parse XLM balance from: $BALANCE_LINE"
fi

if (( XLM_BALANCE < MIN_XLM )); then
  die "Deployer balance is ${XLM_BALANCE} XLM — need at least ${MIN_XLM} XLM to deploy all contracts."
fi

echo "    Balance OK: ${XLM_BALANCE} XLM (minimum ${MIN_XLM} XLM)"

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

WASM_DIR="target/wasm32v1-none/release"

echo ""
echo "==> Building contracts (target: wasm32v1-none, profile: release)..."
cargo build --workspace --target wasm32v1-none --release \
  || die "cargo build failed."

# ---------------------------------------------------------------------------
# Deploy loop — each step validates before continuing
# ---------------------------------------------------------------------------

CONTRACTS=(registration verification progress scout_access)

declare -A CONTRACT_IDS

for name in "${CONTRACTS[@]}"; do
  wasm_src="${WASM_DIR}/scoutchain_${name}.wasm"
  wasm_opt="${WASM_DIR}/scoutchain_${name}.optimized.wasm"

  [[ -f "$wasm_src" ]] || die "WASM not found: $wasm_src — did the build succeed?"

  echo ""
  echo "==> Optimizing $name..."
  stellar contract optimize --wasm "$wasm_src" --wasm-out "$wasm_opt" \
    || die "Optimization failed for $name."

  [[ -f "$wasm_opt" ]] || die "Optimized WASM not produced: $wasm_opt"

  echo "==> Deploying $name to $NETWORK..."
  contract_id=$(stellar contract deploy \
    --wasm "$wasm_opt" \
    --source "$DEPLOYER" \
    --network "$NETWORK" 2>&1) \
    || die "Deploy failed for $name: $contract_id"

  require_nonempty "$contract_id" "$name contract ID"

  CONTRACT_IDS[$name]="$contract_id"
  echo "    $name => $contract_id"
done

# ---------------------------------------------------------------------------
# Write .env.contracts atomically (temp file → rename prevents partial writes)
# ---------------------------------------------------------------------------

ENV_OUT=".env.contracts"
TMP_OUT="${ENV_OUT}.tmp"

{
  echo "REGISTRATION_CONTRACT_ID=${CONTRACT_IDS[registration]}"
  echo "VERIFICATION_CONTRACT_ID=${CONTRACT_IDS[verification]}"
  echo "PROGRESS_CONTRACT_ID=${CONTRACT_IDS[progress]}"
  echo "SCOUT_ACCESS_CONTRACT_ID=${CONTRACT_IDS[scout_access]}"
} > "$TMP_OUT"

mv "$TMP_OUT" "$ENV_OUT"

echo ""
echo "==> All contracts deployed. IDs saved to $ENV_OUT"
cat "$ENV_OUT"
