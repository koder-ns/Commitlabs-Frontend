# CommitLabs Soroban Contracts

Soroban smart contracts backing the CommitLabs liquidity commitment lifecycle.
The `escrow` contract is the primary on-chain component used by the frontend
and backend services to create, fund, release, refund, and dispute
commitments.

## Workspace layout

```text
contracts/
|-- Cargo.toml
`-- escrow/
    |-- Cargo.toml
    `-- src/
        |-- lib.rs
        `-- test.rs
```

## Escrow lifecycle

```text
create_commitment -> fund_escrow -> release
                                  -> refund
                                  -> dispute -> resolve_dispute
```

The contract stores each `Commitment` by id, tracks ownership indexes for
lookup by address, and emits lifecycle events for off-chain consumers.

## Public entrypoints

| Function | Description |
| --- | --- |
| `initialize(admin, token, fee_recipient, safe_default_penalty_bps, balanced_default_penalty_bps, aggressive_default_penalty_bps)` | One-time contract setup. |
| `create_commitment(owner, asset, amount, risk, duration_days, penalty_bps, metadata)` | Create an unfunded commitment with an explicit penalty. |
| `create_commitment_with_default(owner, asset, amount, risk, duration_days)` | Create an unfunded commitment using the configured default risk penalty. |
| `fund_escrow(commitment_id)` | Move the owner funds into escrow and mark the commitment as funded. |
| `release(commitment_id)` | Release principal plus accrued yield after maturity. |
| `refund(commitment_id)` | Return principal minus penalty before maturity. |
| `refund_partial(commitment_id, amount)` | Partially exit a funded commitment. |
| `dispute(commitment_id, caller, reason)` | Freeze a funded commitment and store the dispute record. |
| `resolve_dispute(commitment_id, release_to_owner)` | Admin-only settlement of a disputed commitment. |
| `transfer_ownership(commitment_id, new_owner)` | Move marketplace ownership for funded commitments. |
| `record_attestation(commitment_id, attestor, compliance_score)` | Store a compliance attestation. |
| `deposit_yield_pool(admin, amount)` | Admin-only yield funding for mature releases. |
| `pause()` / `unpause()` | Admin-only emergency write controls. |

## Lifecycle event schema

The backend indexer depends on the lifecycle event topics staying stable.
`contracts/escrow/src/lib.rs` includes an explicit comment on the shared helper
that should not be changed without coordinating an indexer update.

### Stable topic tuple

All primary lifecycle events use the same topic order:

```text
(event_name, owner, commitment_id)
```

- `event_name`: `create_commitment`, `fund_escrow`, `release`, `refund`, `dispute`
- `owner`: the stored commitment owner, even when another authorized actor opens
  the dispute
- `commitment_id`: the unique escrow commitment id

### Event payloads

| Event | Payload fields |
| --- | --- |
| `create_commitment` | `asset`, `amount`, `risk`, `maturity`, `penalty_bps` |
| `fund_escrow` | `asset`, `amount`, `risk` |
| `release` | `asset`, `amount`, `accrued_yield`, `payout`, `risk` |
| `refund` | `asset`, `amount`, `refunded_amount`, `penalty`, `risk` |
| `dispute` | `asset`, `amount`, `risk`, `reason_category`, `reason_text`, `disputed_by` |
| `resolve_dispute` | `asset`, `amount`, `payout`, `penalty`, `risk`, `release_to_owner` |

This schema makes it possible to index by owner/id from topics while still
including risk profile and amount in the event data for downstream analytics.

## Yield model

Accrued yield is computed at commitment creation using annualized basis-point
rates:

- `Safe`: `500` bps
- `Balanced`: `700` bps
- `Aggressive`: `1000` bps

The admin must fund the yield pool before matured releases can pay yield.

## Testing

Run the escrow contract tests from the `contracts/` workspace root:

```bash
cargo test
```

The lifecycle event tests assert:

- stable topic ordering
- stable event names
- risk/amount fields in payloads
- event emission across create, fund, release, refund, and dispute
