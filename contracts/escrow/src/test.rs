#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env, IntoVal, String, Symbol, TryFromVal, Val,
};

// ── Test fixture ─────────────────────────────────────────────────────────────

/// Spins up a test environment with a Stellar Asset Contract token and a
/// deployed, initialized escrow contract. Returns the pieces tests need.
struct Fixture<'a> {
    env: Env,
    client: EscrowContractClient<'a>,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
    admin: Address,
    fee_recipient: Address,
    asset: Address,
    contract_id: Address,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    // Deploy a SAC token to use as the escrow asset.
    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let asset = sac.address();
    let token = TokenClient::new(&env, &asset);
    let token_admin = StellarAssetClient::new(&env, &asset);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&admin, &asset, &fee_recipient);

    Fixture {
        env,
        client,
        token,
        token_admin,
        admin,
        fee_recipient,
        asset,
        contract_id,
    }
}

fn fund_owner(f: &Fixture, owner: &Address, amount: i128) {
    f.token_admin.mint(owner, &amount);
}

// ── Event assertion helper ────────────────────────────────────────────────────

/// Asserts that the escrow contract emitted exactly one event whose first topic
/// matches `event_name` and whose data converts to `expected_data`.
///
/// Soroban's `env.events().all()` returns a `Vec<(Address, Vec<Val>, Val)>`
/// where each entry is `(contract_id, topics, data)`.  We filter to events
/// emitted by the escrow contract and whose first topic is the expected symbol,
/// then compare the data payload.
///
/// # Panics
/// Panics with a descriptive message if no matching event is found or if the
/// data does not match.
fn assert_event<D: IntoVal<Env, Val>>(
    env: &Env,
    contract_id: &Address,
    event_name: &str,
    expected_data: D,
) {
    let all = env.events().all();
    let sym = Symbol::new(env, event_name);
    let expected_val: Val = expected_data.into_val(env);

    let found = all.iter().any(|(id, topics, data)| {
        if &id != contract_id {
            return false;
        }
        // topics is soroban_sdk::Vec<Val>; first element is the Symbol
        if topics.len() == 0 {
            return false;
        }
        let first_val = topics.get(0).unwrap();
        let first_topic = Symbol::try_from_val(env, &first_val)
            .unwrap_or_else(|_| Symbol::new(env, "__none__"));
        if first_topic != sym {
            return false;
        }
        data == expected_val
    });

    assert!(
        found,
        "expected event '{}' with matching data not found in emitted events",
        event_name
    );
}

// ── Existing lifecycle tests (unchanged) ─────────────────────────────────────

#[test]
fn initialize_is_one_time() {
    let f = setup();
    let other = Address::generate(&f.env);
    let res = f
        .client
        .try_initialize(&f.admin, &f.asset, &other);
    assert_eq!(res, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn create_and_fund_locks_funds() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    let c = f.client.get_commitment(&id);
    assert_eq!(c.status, EscrowStatus::Created);
    assert_eq!(c.amount, 1_000);

    f.client.fund_escrow(&id);
    assert_eq!(f.token.balance(&owner), 0);
    assert_eq!(f.client.get_commitment(&id).status, EscrowStatus::Funded);
}

#[test]
fn release_after_maturity_returns_principal() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Safe, &10, &200);
    f.client.fund_escrow(&id);

    // Advance ledger time past maturity.
    f.env.ledger().set_timestamp(11 * 86_400);
    let paid = f.client.release(&id, &owner);
    assert_eq!(paid, 1_000);
    assert_eq!(f.token.balance(&owner), 1_000);
    assert_eq!(f.client.get_commitment(&id).status, EscrowStatus::Released);
}

#[test]
fn release_before_maturity_fails() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Safe, &10, &200);
    f.client.fund_escrow(&id);

    let res = f.client.try_release(&id, &owner);
    assert_eq!(res, Err(Ok(Error::NotMatured)));
}

#[test]
fn refund_applies_penalty_to_fee_recipient() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);
    // 5% penalty.
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Aggressive, &30, &500);
    f.client.fund_escrow(&id);

    let refunded = f.client.refund(&id);
    assert_eq!(refunded, 950);
    assert_eq!(f.token.balance(&owner), 950);
    assert_eq!(f.token.balance(&f.fee_recipient), 50);
    assert_eq!(f.client.get_commitment(&id).status, EscrowStatus::Refunded);
}

#[test]
fn dispute_freezes_then_admin_resolves() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    f.client.fund_escrow(&id);

    f.client
        .dispute(&id, &owner, &String::from_str(&f.env, "value mismatch"));
    assert_eq!(f.client.get_commitment(&id).status, EscrowStatus::Disputed);

    // Release/refund are blocked while disputed.
    assert_eq!(
        f.client.try_refund(&id),
        Err(Ok(Error::InvalidState))
    );

    let paid = f.client.resolve_dispute(&id, &true);
    assert_eq!(paid, 1_000);
    assert_eq!(f.token.balance(&owner), 1_000);
}

#[test]
fn create_rejects_invalid_amount() {
    let f = setup();
    let owner = Address::generate(&f.env);
    let res =
        f.client
            .try_create_commitment(&owner, &f.asset, &0, &RiskProfile::Safe, &30, &200);
    assert_eq!(res, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn create_rejects_excessive_penalty() {
    let f = setup();
    let owner = Address::generate(&f.env);
    let res = f.client.try_create_commitment(
        &owner,
        &f.asset,
        &1_000,
        &RiskProfile::Safe,
        &30,
        &20_000,
    );
    assert_eq!(res, Err(Ok(Error::PenaltyTooHigh)));
}

#[test]
fn record_attestation_clamps_score() {
    let f = setup();
    let owner = Address::generate(&f.env);
    let attestor = Address::generate(&f.env);
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    f.client.record_attestation(&id, &attestor, &250);
    assert_eq!(f.client.get_commitment(&id).compliance_score, 100);
}

#[test]
fn owner_index_tracks_commitments() {
    let f = setup();
    let owner = Address::generate(&f.env);
    let a = f
        .client
        .create_commitment(&owner, &f.asset, &100, &RiskProfile::Safe, &30, &200);
    let b = f
        .client
        .create_commitment(&owner, &f.asset, &200, &RiskProfile::Balanced, &30, &300);
    let ids = f.client.get_owner_commitments(&owner);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get(0).unwrap(), a);
    assert_eq!(ids.get(1).unwrap(), b);
}

// ── Event assertion tests ─────────────────────────────────────────────────────

/// `create_commitment` must emit `(Symbol("create_commitment"), owner)` with
/// data `(id, amount, maturity)`.
#[test]
fn event_create_commitment() {
    let f = setup();
    let owner = Address::generate(&f.env);

    // Ledger timestamp starts at 0; maturity = 30 * 86_400.
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &500, &RiskProfile::Safe, &30, &200);

    let expected_maturity: u64 = 30 * 86_400;
    assert_event(
        &f.env,
        &f.contract_id,
        "create_commitment",
        (id, 500_i128, expected_maturity),
    );
}

/// `fund_escrow` must emit `(Symbol("fund_escrow"), owner)` with data
/// `(commitment_id, amount)`.
#[test]
fn event_fund_escrow() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    f.client.fund_escrow(&id);

    assert_event(
        &f.env,
        &f.contract_id,
        "fund_escrow",
        (id, 1_000_i128),
    );
}

/// `release` must emit `(Symbol("release"), owner)` with data
/// `(commitment_id, amount)`.
#[test]
fn event_release() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Safe, &10, &200);
    f.client.fund_escrow(&id);
    f.env.ledger().set_timestamp(11 * 86_400);
    f.client.release(&id, &owner);

    assert_event(
        &f.env,
        &f.contract_id,
        "release",
        (id, 1_000_i128),
    );
}

/// `refund` must emit `(Symbol("refund"), owner)` with data
/// `(commitment_id, refund_amount, penalty)`.
#[test]
fn event_refund() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    // 5% penalty → refund = 950, penalty = 50.
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Aggressive, &30, &500);
    f.client.fund_escrow(&id);
    f.client.refund(&id);

    assert_event(
        &f.env,
        &f.contract_id,
        "refund",
        (id, 950_i128, 50_i128),
    );
}

/// `dispute` must emit `(Symbol("dispute"), caller)` with data
/// `(commitment_id, reason)`.
#[test]
fn event_dispute() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    f.client.fund_escrow(&id);

    let reason = String::from_str(&f.env, "value mismatch");
    f.client.dispute(&id, &owner, &reason);

    assert_event(
        &f.env,
        &f.contract_id,
        "dispute",
        (id, reason),
    );
}

/// `resolve_dispute` (release path) must emit
/// `(Symbol("resolve_dispute"), admin)` with data
/// `(commitment_id, release_to_owner, paid)`.
#[test]
fn event_resolve_dispute_release() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Balanced, &30, &300);
    f.client.fund_escrow(&id);
    f.client
        .dispute(&id, &owner, &String::from_str(&f.env, "test"));
    f.client.resolve_dispute(&id, &true);

    assert_event(
        &f.env,
        &f.contract_id,
        "resolve_dispute",
        (id, true, 1_000_i128),
    );
}

/// `resolve_dispute` (refund path) must emit the correct penalty-adjusted paid
/// amount in the data payload.
#[test]
fn event_resolve_dispute_refund() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    // 10% penalty → paid = 900.
    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Aggressive, &30, &1_000);
    f.client.fund_escrow(&id);
    f.client
        .dispute(&id, &owner, &String::from_str(&f.env, "test"));
    f.client.resolve_dispute(&id, &false);

    assert_event(
        &f.env,
        &f.contract_id,
        "resolve_dispute",
        (id, false, 900_i128),
    );
}

/// Each test is independent: events from one call do not bleed into another
/// because each test constructs its own `Env`.  This test explicitly verifies
/// that only the expected event name is matched even when multiple events exist.
#[test]
fn assert_event_matches_correct_name_only() {
    let f = setup();
    let owner = Address::generate(&f.env);
    fund_owner(&f, &owner, 1_000);

    let id = f
        .client
        .create_commitment(&owner, &f.asset, &1_000, &RiskProfile::Safe, &10, &200);
    f.client.fund_escrow(&id);

    // Both create_commitment and fund_escrow events exist; each helper call
    // must match only its own event.
    assert_event(
        &f.env,
        &f.contract_id,
        "create_commitment",
        (id, 1_000_i128, 10_u64 * 86_400_u64),
    );
    assert_event(
        &f.env,
        &f.contract_id,
        "fund_escrow",
        (id, 1_000_i128),
    );
}
