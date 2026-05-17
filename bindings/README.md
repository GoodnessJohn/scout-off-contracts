# ScoutChain — Generated Contract Bindings

TypeScript clients auto-generated from the deployed WASM contracts.
These are consumed by `scoutchain-backend` and `scoutchain-frontend`.

## Regenerate

After deploying contracts, run:

```bash
./scripts/generate-bindings.sh testnet
```

This calls `stellar contract bindings typescript` for each contract and writes
the output into the subdirectories below.

## Structure

```
bindings/
  registration/     ← TypeScript client for the registration contract
  verification/     ← TypeScript client for the verification contract
  progress/         ← TypeScript client for the progress contract
  scout_access/     ← TypeScript client for the scout_access contract
```

## Usage in backend / frontend

```typescript
import { Client as RegistrationClient } from "@scoutchain/bindings-registration";
import { Client as ProgressClient }     from "@scoutchain/bindings-progress";
```

Install from the local path or publish to a private npm registry.
