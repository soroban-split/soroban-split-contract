#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype,
    token::Client as TokenClient,
    Address, Env, Vec,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Single persistent key for the split configuration.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Config,
}

// ---------------------------------------------------------------------------
// Error taxonomy
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Error {
    /// `initialize` was never called.
    NotInitialized = 1,
    /// `initialize` was already called; the contract is immutable after init.
    AlreadyInitialized = 2,
    /// Weights do not sum to exactly 10 000 basis points.
    InvalidWeights = 3,
    /// A zero `total_amount` was supplied to `distribute_tokens`.
    ZeroAmount = 4,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A single recipient and their proportional share expressed in basis points
/// (1 bp = 0.01 %).  The sum of all `weight` values in a valid config is
/// exactly 10 000, representing 100.00 %.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    pub contributor: Address,
    pub weight: u32,
}

/// The immutable configuration stored on-chain after `initialize`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitConfig {
    pub owner: Address,
    pub shares: Vec<Share>,
}

// ---------------------------------------------------------------------------
// Total basis points that weights must sum to (100.00 %)
// ---------------------------------------------------------------------------

const BASIS_POINTS_TOTAL: u32 = 10_000;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct SorobanSplitContract;

#[contractimpl]
impl SorobanSplitContract {
    // -----------------------------------------------------------------------
    // initialize
    // -----------------------------------------------------------------------

    /// Store the split configuration.  Can only be called once.
    ///
    /// # Errors
    /// - [`Error::AlreadyInitialized`] — called more than once.
    /// - [`Error::InvalidWeights`]    — weights do not sum to 10 000 bp.
    pub fn initialize(
        env: Env,
        owner: Address,
        shares: Vec<Share>,
    ) -> Result<(), Error> {
        // Single-initialization guard.
        if env
            .storage()
            .persistent()
            .has(&DataKey::Config)
        {
            return Err(Error::AlreadyInitialized);
        }

        // Require the caller to be the declared owner.
        owner.require_auth();

        // Validate that weights sum to exactly BASIS_POINTS_TOTAL.
        let mut total_weight: u64 = 0;
        for share in shares.iter() {
            total_weight = total_weight
                .checked_add(share.weight as u64)
                .ok_or(Error::InvalidWeights)?;
        }
        if total_weight != BASIS_POINTS_TOTAL as u64 {
            return Err(Error::InvalidWeights);
        }

        let config = SplitConfig { owner, shares };
        env.storage()
            .persistent()
            .set(&DataKey::Config, &config);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // distribute_tokens
    // -----------------------------------------------------------------------

    /// Transfer `total_amount` of `token_id` tokens held by this contract to
    /// each contributor proportionally.
    ///
    /// Arithmetic uses the scale:
    ///   contributor_amount = (total_amount * weight) / BASIS_POINTS_TOTAL
    ///
    /// Because integer division truncates, any dust (≤ number-of-shares − 1
    /// stroops) remains in the contract, keeping the invariant that we never
    /// over-distribute.
    ///
    /// # Errors
    /// - [`Error::NotInitialized`] — `initialize` was never called.
    /// - [`Error::ZeroAmount`]     — `total_amount` is zero or negative.
    pub fn distribute_tokens(
        env: Env,
        token_id: Address,
        total_amount: i128,
    ) -> Result<(), Error> {
        if total_amount <= 0 {
            return Err(Error::ZeroAmount);
        }

        let config: SplitConfig = env
            .storage()
            .persistent()
            .get(&DataKey::Config)
            .ok_or(Error::NotInitialized)?;

        let token = TokenClient::new(&env, &token_id);
        let contract_address = env.current_contract_address();

        for share in config.shares.iter() {
            // Safe scale arithmetic — no risk of overflow for realistic i128
            // amounts and u32 weights, but we use i128 multiplication
            // explicitly.
            let contributor_amount: i128 =
                total_amount
                    .checked_mul(share.weight as i128)
                    .expect("overflow in weight scale")
                    / BASIS_POINTS_TOTAL as i128;

            if contributor_amount > 0 {
                token.transfer(
                    &contract_address,
                    &share.contributor,
                    &contributor_amount,
                );
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // get_config
    // -----------------------------------------------------------------------

    /// Read-only accessor for the stored split configuration.
    /// Returns `None` before `initialize` is called.
    pub fn get_config(env: Env) -> Option<SplitConfig> {
        env.storage()
            .persistent()
            .get(&DataKey::Config)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
