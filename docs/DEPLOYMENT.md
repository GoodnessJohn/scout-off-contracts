# Deployment Guide

## Prerequisites

- Rust + `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- Stellar CLI: https://developers.stellar.org/docs/tools/developer-tools/cli/install-stellar-cli
- A funded Stellar keypair for deployment

## Step-by-step

### 1. Configure environment

```bash
cp .env.example .env
# Fill in DEPLOYER_SECRET, ADMIN_ADDRESS, XLM_TOKEN_ADDRESS
```

### 2. Deploy all contracts

```bash
chmod +x scripts/deploy.sh
./scripts/deploy.sh testnet
# Contract IDs written to .env.contracts
```

### 3. Initialize and wire contracts

```bash
chmod +x scripts/initialize.sh
./scripts/initialize.sh testnet
# Sets admin, fee config, and wires verification → progress cross-contract link
```

### 4. Generate TypeScript bindings

```bash
chmod +x scripts/generate-bindings.sh
./scripts/generate-bindings.sh testnet
# Bindings written to bindings/{contract}/
```

### 5. Seed testnet with demo data (optional)

```bash
chmod +x testnet/seed.sh
./testnet/seed.sh
```

### 6. Run the database migration

Copy `migrations/001_initial_schema.sql` to your backend repo and run it against PostgreSQL:

```bash
psql $DATABASE_URL -f migrations/001_initial_schema.sql
```

## Mainnet checklist

- [ ] Audit all four contracts
- [ ] Replace testnet XLM token address with mainnet address in `.env`
- [ ] Set `STELLAR_NETWORK=mainnet` and update RPC/Horizon URLs
- [ ] Run `./scripts/deploy.sh mainnet`
- [ ] Run `./scripts/initialize.sh mainnet`
- [ ] Verify all contract IDs in `.env.contracts`
- [ ] Regenerate bindings: `./scripts/generate-bindings.sh mainnet`
