# CommitLabs Soroban Contracts

Soroban (Rust) smart-contract workspace backing the CommitLabs liquidity
commitment protocol. The frontend and Next.js backend service layer
(`src/lib/backend/services/contracts.ts`) interact with these contracts via the
Stellar Soroban RPC.

## Workspace layout

```
contracts/
├── Cargo.toml          # Cargo workspace (members = ["escrow"])
└── escrow/
    ├── Cargo.toml      # commitlabs-escrow crate (cdylib + rlib)
    └── src/
        ├── lib.rs      # EscrowContract implementation
        └── test.rs     # Unit tests (cfg(test))
```

## `escrow` contract

The escrow contract manages the on-chain lifecycle of a liquidity commitment.
Assets are deposited under a chosen risk profile and held in escrow until the
commitment matures, is exited early, or is disputed.

### Lifecycle

```
create_commitment ──► fund_escrow ──► release            (matured: principal back to owner)
                                  └──► refund             (early exit: principal − penalty)
                                  └──► dispute ──► resolve_dispute   (admin adjudication)
```

### Public functions

| Function | Description |
| --- | --- |
| `initialize(admin, token, fee_recipient)` | One-time setup of admin, escrow token (SAC) and penalty fee recipient. |
| `create_commitment(owner, asset, amount, risk, duration_days, penalty_bps)` | Create an unfunded commitment; returns its `id`. |
| `fund_escrow(commitment_id)` | Transfer `amount` from owner into the contract (`Created → Funded`). |
| `release(commitment_id, caller)` | Return principal to owner once matured (`Funded → Released`). |
| `refund(commitment_id)` | Early-exit refund of principal minus `penalty_bps` (`Funded → Refunded`). |
| `dispute(commitment_id, caller, reason)` | Freeze a funded commitment pending admin resolution. |
| `resolve_dispute(commitment_id, release_to_owner)` | Admin-only settlement of a disputed commitment. |
| `record_attestation(commitment_id, attestor, compliance_score)` | Record a 0–100 compliance score. |
| `get_commitment(commitment_id)` | Read a single commitment record. |
| `get_owner_commitments(owner)` | List commitment ids owned by an address. |

### Risk profiles & penalties

`RiskProfile` is `Safe | Balanced | Aggressive`, matching the frontend
`CommitmentType`. The early-exit penalty is supplied at creation time in basis
points (`penalty_bps`, max `10_000`) and is paid to the configured fee
recipient on `refund` / adverse `resolve_dispute`.

### Errors

Stable numeric error codes (`#[contracterror]`) are surfaced so the backend
`normalizeContractError` mapper can translate them into HTTP responses:
`AlreadyInitialized`, `NotInitialized`, `NotFound`, `Unauthorized`,
`InvalidAmount`, `InvalidState`, `NotMatured`, `InvalidDuration`,
`PenaltyTooHigh`.

## Build & test

Requires the `stellar` CLI (v23) and the `wasm32v1-none` / `wasm32-unknown-unknown`
target.

```bash
# from contracts/
cargo test            # run unit tests in escrow/src/test.rs
stellar contract build
```

> Note: this workspace is scaffolded to ground the contract issue backlog.
> Verify a local toolchain before deploying to testnet/mainnet.

## Event-assertion tests

`escrow/src/test.rs` includes an `assert_event` helper and dedicated tests that
verify every lifecycle function emits the expected Soroban event.

### Helper: `assert_event`

```rust
fn assert_event<D: IntoVal<Env, Val>>(
    env: &Env,
    contract_id: &Address,
    event_name: &str,
    expected_data: D,
)
```

Reads `env.events().all()` (the `soroban_sdk::testutils::Events` trait), filters
to events emitted by the escrow contract whose first topic matches `event_name`,
and asserts that the data payload equals `expected_data`. Panics with a
descriptive message on failure.

### Event coverage

| Function | Topics | Data |
| --- | --- | --- |
| `create_commitment` | `(Symbol("create_commitment"), owner)` | `(id, amount, maturity)` |
| `fund_escrow` | `(Symbol("fund_escrow"), owner)` | `(commitment_id, amount)` |
| `release` | `(Symbol("release"), owner)` | `(commitment_id, amount)` |
| `refund` | `(Symbol("refund"), owner)` | `(commitment_id, refund_amount, penalty)` |
| `dispute` | `(Symbol("dispute"), caller)` | `(commitment_id, reason)` |
| `resolve_dispute` | `(Symbol("resolve_dispute"), admin)` | `(commitment_id, release_to_owner, paid)` |

### Test list

- `event_create_commitment` — asserts `(id, amount, maturity)` after `create_commitment`.
- `event_fund_escrow` — asserts `(id, amount)` after `fund_escrow`.
- `event_release` — asserts `(id, amount)` after `release` past maturity.
- `event_refund` — asserts `(id, refund_amount, penalty)` with 5% penalty.
- `event_dispute` — asserts `(id, reason)` after `dispute`.
- `event_resolve_dispute_release` — asserts `(id, true, paid)` on admin release.
- `event_resolve_dispute_refund` — asserts `(id, false, paid)` on admin refund with penalty.
- `assert_event_matches_correct_name_only` — verifies the helper matches only the
  named event when multiple events exist in the same environment.

### Design decisions

- **Independent environments**: each test constructs its own `Env::default()`, so
  events never bleed between tests.
- **Deterministic maturity**: ledger timestamp starts at 0 in tests, so
  `maturity = duration_days * 86_400` is always predictable.
- **Data comparison via `IntoVal`**: the helper converts the expected tuple to a
  `Val` using the same `IntoVal<Env, Val>` path the SDK uses internally, so the
  comparison is byte-for-byte identical to what the contract emits.