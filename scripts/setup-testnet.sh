#!/usr/bin/env bash
# ScoutChain — full testnet setup in one command.
# Runs: deploy → initialize → generate-bindings → seed
#
# Idempotent: any existing .env.contracts is removed before starting so a
# second run always produces a clean, consistent deployment state.
#
# Usage: ./scripts/setup-testnet.sh
#
# Requires .env to exist (copy from .env.example and fill in values).
set -euo pipefail

# ---------------------------------------------------------------------------
# Helper
# ---------------------------------------------------------------------------

die() { echo "ERROR: $*" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ---------------------------------------------------------------------------
# Pre-flight: .env must exist and required vars must be set
# ---------------------------------------------------------------------------

[[ -f "$REPO_ROOT/.env" ]] \
  || die ".env not found. Copy .env.example to .env and fill in DEPLOYER_SECRET, ADMIN_ADDRESS, and XLM_TOKEN_ADDRESS."

# shellcheck source=/dev/null
source "$REPO_ROOT/.env"

[[ -n "${DEPLOYER_SECRET:-}" ]]   || die "DEPLOYER_SECRET is not set in .env."
[[ -n "${ADMIN_ADDRESS:-}" ]]     || die "ADMIN_ADDRESS is not set in .env."
[[ -n "${XLM_TOKEN_ADDRESS:-}" ]] || die "XLM_TOKEN_ADDRESS is not set in .env."

# ---------------------------------------------------------------------------
# Idempotency: wipe any previous partial deployment before starting
# ---------------------------------------------------------------------------

if [[ -f "$REPO_ROOT/.env.contracts" ]]; then
  echo "==> Found existing .env.contracts — removing for clean re-deployment."
  rm "$REPO_ROOT/.env.contracts"
fi

# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------

echo ""
echo "========================================"
echo "  ScoutChain Testnet Setup"
echo "========================================"
echo ""

cd "$REPO_ROOT"

echo "Step 1/4 — Deploy contracts"
bash scripts/deploy.sh testnet \
  || die "deploy.sh failed — see output above."

echo ""
echo "Step 2/4 — Initialize contracts"
bash scripts/initialize.sh testnet \
  || die "initialize.sh failed — see output above."

echo ""
echo "Step 3/4 — Generate TypeScript bindings"
bash scripts/generate-bindings.sh testnet \
  || die "generate-bindings.sh failed — see output above."

echo ""
echo "Step 4/4 — Seed demo data"
bash testnet/seed.sh \
  || die "seed.sh failed — see output above."

echo ""
echo "========================================"
echo "  Setup complete!"
echo "  Contract IDs : .env.contracts"
echo "  Test accounts: testnet/.accounts"
echo "  Bindings     : bindings/"
echo "========================================"
