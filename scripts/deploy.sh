#!/usr/bin/env bash
# ScoutChain — deploy all contracts to Stellar testnet or mainnet
# Usage: ./scripts/deploy.sh [testnet|mainnet]
set -euo pipefail

NETWORK="${1:-testnet}"
DEPLOYER="${DEPLOYER_SECRET:-}"

if [[ -z "$DEPLOYER" ]]; then
  echo "ERROR: Set DEPLOYER_SECRET env var to your Stellar secret key."
  exit 1
fi

# Validate mainnet config before proceeding
if [[ "$NETWORK" == "mainnet" ]]; then
  echo "⚠️  MAINNET DEPLOYMENT DETECTED ⚠️"
  echo ""
  echo "Checking config/mainnet.json for placeholder values..."
  if grep -q "FILL_IN_BEFORE_USE\|YOUR_API_KEY" config/mainnet.json; then
    echo "ERROR: config/mainnet.json contains placeholder values (FILL_IN_BEFORE_USE or YOUR_API_KEY)"
    echo "Please update the file with real values before deploying to mainnet:"
    echo "  - Replace YOUR_API_KEY with your actual Validation Cloud API key (or another RPC provider)"
    echo "  - Verify xlm_token_address is correct for mainnet"
    echo ""
    exit 1
  fi
  echo "✓ config/mainnet.json validated — no placeholder values found."
  echo ""
fi

WASM_DIR="target/wasm32-unknown-unknown/release"

echo "==> Building contracts..."
cargo build --workspace --target wasm32-unknown-unknown --release

CONTRACTS=(registration verification progress scout_access)

declare -A CONTRACT_IDS

for name in "${CONTRACTS[@]}"; do
  wasm_name="scoutchain_${name}.wasm"
  optimized="${WASM_DIR}/scoutchain_${name}.optimized.wasm"

  echo "==> Optimizing $name..."
  stellar contract optimize --wasm "${WASM_DIR}/${wasm_name}" --wasm-out "$optimized"

  echo "==> Deploying $name to $NETWORK..."
  id=$(stellar contract deploy \
    --wasm "$optimized" \
    --source "$DEPLOYER" \
    --network "$NETWORK")

  CONTRACT_IDS[$name]="$id"
  echo "    $name => $id"
done

# Write contract IDs to .env.contracts
{
  echo "REGISTRATION_CONTRACT_ID=${CONTRACT_IDS[registration]}"
  echo "VERIFICATION_CONTRACT_ID=${CONTRACT_IDS[verification]}"
  echo "PROGRESS_CONTRACT_ID=${CONTRACT_IDS[progress]}"
  echo "SCOUT_ACCESS_CONTRACT_ID=${CONTRACT_IDS[scout_access]}"
} > .env.contracts

echo ""
echo "==> All contracts deployed. IDs saved to .env.contracts"
