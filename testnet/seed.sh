#!/usr/bin/env bash
# ScoutChain — seed testnet with demo data
# Run after initialize.sh to create test players, validators, and scouts.
set -euo pipefail

source .env.contracts

NETWORK="testnet"
DEPLOYER="${DEPLOYER_SECRET:?Set DEPLOYER_SECRET}"
ADMIN="${ADMIN_ADDRESS:?Set ADMIN_ADDRESS}"

echo "==> Generating test keypairs..."

PLAYER_SECRET=$(stellar keys generate --no-fund player-test 2>/dev/null || stellar keys show player-test --secret)
SCOUT_SECRET=$(stellar keys generate --no-fund scout-test 2>/dev/null || stellar keys show scout-test --secret)
VALIDATOR_SECRET=$(stellar keys generate --no-fund validator-test 2>/dev/null || stellar keys show validator-test --secret)

PLAYER_ADDRESS=$(stellar keys address player-test)
SCOUT_ADDRESS=$(stellar keys address scout-test)
VALIDATOR_ADDRESS=$(stellar keys address validator-test)

echo "    Player:    $PLAYER_ADDRESS"
echo "    Scout:     $SCOUT_ADDRESS"
echo "    Validator: $VALIDATOR_ADDRESS"

echo "==> Funding test accounts via Friendbot..."
curl -s "https://friendbot.stellar.org?addr=$PLAYER_ADDRESS"    > /dev/null
curl -s "https://friendbot.stellar.org?addr=$SCOUT_ADDRESS"     > /dev/null
curl -s "https://friendbot.stellar.org?addr=$VALIDATOR_ADDRESS" > /dev/null

echo "==> Registering validator..."
stellar contract invoke \
  --id "$VERIFICATION_CONTRACT_ID" \
  --source "$DEPLOYER" \
  --network "$NETWORK" \
  -- register_validator \
  --wallet "$VALIDATOR_ADDRESS" \
  --credentials "UEFA B License — Test Validator"

echo "==> Registering test player..."
stellar contract invoke \
  --id "$REGISTRATION_CONTRACT_ID" \
  --source player-test \
  --network "$NETWORK" \
  -- register_player \
  --wallet "$PLAYER_ADDRESS" \
  --vitals '{"age":19,"position":"Forward","region":"West Africa","nationality":"Ghana"}' \
  --ipfs_hashes '["QmTestHighlight1","QmTestPhoto1"]'

echo "==> Registering test scout..."
stellar contract invoke \
  --id "$REGISTRATION_CONTRACT_ID" \
  --source scout-test \
  --network "$NETWORK" \
  -- register_scout \
  --wallet "$SCOUT_ADDRESS" \
  --region "Europe"

echo ""
echo "==> Seed complete."
echo "    Player address:    $PLAYER_ADDRESS"
echo "    Scout address:     $SCOUT_ADDRESS"
echo "    Validator address: $VALIDATOR_ADDRESS"
echo ""
echo "    Save these to testnet/.accounts for reference."

{
  echo "PLAYER_ADDRESS=$PLAYER_ADDRESS"
  echo "SCOUT_ADDRESS=$SCOUT_ADDRESS"
  echo "VALIDATOR_ADDRESS=$VALIDATOR_ADDRESS"
} > testnet/.accounts
