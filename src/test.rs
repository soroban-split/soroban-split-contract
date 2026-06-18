//! Integration tests for SorobanSplitContract.
//!
//! Each test spins up a fresh [`soroban_sdk::Env`], registers the split
//! contract and a Stellar Asset Contract (SAC) mock token, then exercises the
//! public API end-to-end.

#![cfg(test)]

extern crate std;

use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Vec,
};

use crate::{Share, SorobanSplitContract, SorobanSplitContractClient, SplitConfig};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register the split contract and return its client.
fn deploy_split(env: &Env) -> SorobanSplitContractClient {
    let contract_id = env.register(SorobanSplitContract, ());
    SorobanSplitContractClient::new(env, &contract_id)
}

/// Register a mock SAC token, returning (token_address, admin_client).
fn deploy_token(env: &Env) -> (Address, StellarAssetClient) {
    let admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let sac = StellarAssetClient::new(env, &token_id.address());
    (token_id.address(), sac)
}

/// Build the three-share Vec used across multiple tests.
fn three_shares(env: &Env, a: &Address, b: &Address, c: &Address) -> Vec<Share> {
    // 5 000 bp (50 %), 3 000 bp (30 %), 2 000 bp (20 %) — sums to 10 000.
    soroban_sdk::vec![
        env,
        Share { contributor: a.clone(), weight: 5_000 },
        Share { contributor: b.clone(), weight: 3_000 },
        Share { contributor: c.clone(), weight: 2_000 },
    ]
}

// ---------------------------------------------------------------------------
// Test: happy-path end-to-end distribution
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_exact_amounts() {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy contracts.
    let split = deploy_split(&env);
    let (token_id, sac) = deploy_token(&env);

    // Three contributors.
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    // Initialize the split.
    let shares = three_shares(&env, &alice, &bob, &carol);
    split
        .initialize(&Address::generate(&env), &shares)
        .unwrap();

    // Mint 10 000 tokens to the split contract so it can distribute.
    let total: i128 = 10_000;
    sac.mint(&split.address, &total);
    assert_eq!(TokenClient::new(&env, &token_id).balance(&split.address), total);

    // Distribute.
    split.distribute_tokens(&token_id, &total).unwrap();

    // Verify exact splits.
    let token = TokenClient::new(&env, &token_id);
    assert_eq!(token.balance(&alice), 5_000, "Alice should receive 50 %");
    assert_eq!(token.balance(&bob),   3_000, "Bob should receive 30 %");
    assert_eq!(token.balance(&carol), 2_000, "Carol should receive 20 %");
    // Contract balance should be zero (no dust for a clean divisible amount).
    assert_eq!(token.balance(&split.address), 0, "Contract should be empty");
}

// ---------------------------------------------------------------------------
// Test: dust remains in contract for non-divisible amounts
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_with_dust() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let (token_id, sac) = deploy_token(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    split
        .initialize(&Address::generate(&env), &three_shares(&env, &alice, &bob, &carol))
        .unwrap();

    // 10 001 is not evenly divisible by the 20 % slice (2 000.2).
    let total: i128 = 10_001;
    sac.mint(&split.address, &total);

    split.distribute_tokens(&token_id, &total).unwrap();

    let token = TokenClient::new(&env, &token_id);
    // 50 % of 10 001 = 5 000 (truncated)
    assert_eq!(token.balance(&alice), 5_000);
    // 30 % of 10 001 = 3 000 (truncated)
    assert_eq!(token.balance(&bob),   3_000);
    // 20 % of 10 001 = 2 000 (truncated)
    assert_eq!(token.balance(&carol), 2_000);
    // Dust = 10 001 − 5 000 − 3 000 − 2 000 = 1 stays in contract.
    assert_eq!(token.balance(&split.address), 1);
}

// ---------------------------------------------------------------------------
// Test: get_config returns None before init, Some after
// ---------------------------------------------------------------------------

#[test]
fn test_get_config_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    assert!(split.get_config().is_none());

    let owner = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let shares = soroban_sdk::vec![
        &env,
        Share { contributor: alice.clone(), weight: 6_000 },
        Share { contributor: bob.clone(),   weight: 4_000 },
    ];

    split.initialize(&owner, &shares).unwrap();

    let config = split.get_config().expect("config should exist after init");
    assert_eq!(config.owner, owner);
    assert_eq!(config.shares.len(), 2);
}

// ---------------------------------------------------------------------------
// Test: double-initialize returns AlreadyInitialized
// ---------------------------------------------------------------------------

#[test]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let owner = Address::generate(&env);
    let alice = Address::generate(&env);

    let shares = soroban_sdk::vec![
        &env,
        Share { contributor: alice.clone(), weight: 10_000 },
    ];

    split.initialize(&owner, &shares.clone()).unwrap();

    let result = split.try_initialize(&owner, &shares);
    assert!(
        result.is_err(),
        "second initialize must return an error"
    );
}

// ---------------------------------------------------------------------------
// Test: invalid weights (sum ≠ 10 000) returns InvalidWeights
// ---------------------------------------------------------------------------

#[test]
fn test_invalid_weights_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let owner = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Weights sum to 9 999, not 10 000.
    let bad_shares = soroban_sdk::vec![
        &env,
        Share { contributor: alice.clone(), weight: 5_000 },
        Share { contributor: bob.clone(),   weight: 4_999 },
    ];

    let result = split.try_initialize(&owner, &bad_shares);
    assert!(result.is_err(), "invalid weights must be rejected");
}

// ---------------------------------------------------------------------------
// Test: zero amount returns ZeroAmount
// ---------------------------------------------------------------------------

#[test]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let (token_id, _sac) = deploy_token(&env);
    let alice = Address::generate(&env);

    let shares = soroban_sdk::vec![
        &env,
        Share { contributor: alice.clone(), weight: 10_000 },
    ];
    split.initialize(&Address::generate(&env), &shares).unwrap();

    let result = split.try_distribute_tokens(&token_id, &0_i128);
    assert!(result.is_err(), "zero amount must be rejected");
}

// ---------------------------------------------------------------------------
// Test: distribute before init returns NotInitialized
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let (token_id, _sac) = deploy_token(&env);

    let result = split.try_distribute_tokens(&token_id, &1_000_i128);
    assert!(result.is_err(), "distributing before init must fail");
}

// ---------------------------------------------------------------------------
// Test: single recipient receives 100 % of tokens
// ---------------------------------------------------------------------------

#[test]
fn test_single_recipient_receives_all() {
    let env = Env::default();
    env.mock_all_auths();

    let split = deploy_split(&env);
    let (token_id, sac) = deploy_token(&env);
    let alice = Address::generate(&env);

    let shares = soroban_sdk::vec![
        &env,
        Share { contributor: alice.clone(), weight: 10_000 },
    ];
    split.initialize(&Address::generate(&env), &shares).unwrap();

    let total: i128 = 999_999;
    sac.mint(&split.address, &total);
    split.distribute_tokens(&token_id, &total).unwrap();

    assert_eq!(
        TokenClient::new(&env, &token_id).balance(&alice),
        total,
        "sole recipient must receive entire balance"
    );
}
