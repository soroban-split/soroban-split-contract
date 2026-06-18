# SorobanSplit

A production-ready token-splitting protocol built on [Stellar Soroban](https://soroban.stellar.org). SorobanSplit lets you define a fixed set of contributors and their percentage shares (in basis points), then distribute any Stellar token proportionally across all of them in a single transaction.

Built for the **[Drips Wave](https://drips.network)** program.

---

## Overview

```
Owner deploys & initializes contract with contributor shares
         ↓
Anyone sends tokens to the contract address
         ↓
distribute_tokens() splits them proportionally to each contributor
```

Weights are expressed in **basis points** (1 bp = 0.01%), with all weights required to sum to exactly **10,000 bp = 100.00%**. Distribution arithmetic truncates toward zero — any dust (at most `n_shares − 1` stroops) stays in the contract and is never over-distributed.

---

## Project Structure

```
soroban-split-contract/
├── .cargo/
│   └── config.toml       # Default WASM build target
├── src/
│   ├── lib.rs            # Contract logic
│   └── test.rs           # Integration tests
└── Cargo.toml            # Package manifest & release profile
```

---

## Contract API

### `initialize(env, owner, shares)`

Sets up the split configuration. Can only be called **once** — the contract is immutable after initialization.

| Parameter | Type         | Description                                      |
|-----------|--------------|--------------------------------------------------|
| `owner`   | `Address`    | The deploying owner; must sign the transaction   |
| `shares`  | `Vec<Share>` | List of contributors and their basis-point weights |

**`Share` struct**

```rust
pub struct Share {
    pub contributor: Address,  // Recipient wallet
    pub weight: u32,           // Basis points (e.g. 5000 = 50.00%)
}
```

Errors: `AlreadyInitialized`, `InvalidWeights`

---

### `distribute_tokens(env, token_id, total_amount)`

Transfers `total_amount` of the specified token from the contract to each contributor according to their weight.

| Parameter      | Type      | Description                            |
|----------------|-----------|----------------------------------------|
| `token_id`     | `Address` | The Stellar Asset Contract token address |
| `total_amount` | `i128`    | Amount to distribute (must be > 0)     |

Each contributor receives:
```
amount = (total_amount × weight) / 10_000
```

Errors: `NotInitialized`, `ZeroAmount`

---

### `get_config(env) → Option<SplitConfig>`

Read-only query returning the stored configuration, or `None` if uninitialized.

```rust
pub struct SplitConfig {
    pub owner: Address,
    pub shares: Vec<Share>,
}
```

---

## Error Codes

| Code | Variant              | Meaning                                      |
|------|----------------------|----------------------------------------------|
| 1    | `NotInitialized`     | `initialize` has not been called yet         |
| 2    | `AlreadyInitialized` | `initialize` was already called              |
| 3    | `InvalidWeights`     | Weights do not sum to exactly 10,000 bp      |
| 4    | `ZeroAmount`         | `total_amount` is zero or negative           |

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs) with the `wasm32-unknown-unknown` target
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli/install-stellar-cli)

```bash
rustup target add wasm32-unknown-unknown
```

### Build

```bash
# Development check (native target, fast)
cargo check

# Production WASM binary
cargo build --release --target wasm32-unknown-unknown
```

Output: `target/wasm32-unknown-unknown/release/soroban_split_contract.wasm`

### Test

```bash
cargo test
```

The test suite covers:

| Test | Description |
|------|-------------|
| `test_distribute_exact_amounts` | 3-way split with clean divisible amount |
| `test_distribute_with_dust` | Dust remainder stays in contract |
| `test_get_config_lifecycle` | Config is `None` before init, `Some` after |
| `test_double_initialize_fails` | Second `initialize` returns `AlreadyInitialized` |
| `test_invalid_weights_rejected` | Weights summing to ≠ 10,000 are rejected |
| `test_zero_amount_rejected` | Zero amount returns `ZeroAmount` |
| `test_distribute_before_init_fails` | Distribute before init returns `NotInitialized` |
| `test_single_recipient_receives_all` | 100% weight recipient gets entire balance |

### Deploy (Testnet)

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/soroban_split_contract.wasm \
  --source <YOUR_ACCOUNT> \
  --network testnet
```

### Initialize On-Chain

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <OWNER_ACCOUNT> \
  --network testnet \
  -- initialize \
  --owner <OWNER_ADDRESS> \
  --shares '[{"contributor":"<ADDR_A>","weight":5000},{"contributor":"<ADDR_B>","weight":3000},{"contributor":"<ADDR_C>","weight":2000}]'
```

### Distribute Tokens

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <CALLER> \
  --network testnet \
  -- distribute_tokens \
  --token_id <TOKEN_ADDRESS> \
  --total_amount 10000
```

---

## Release Profile

The WASM binary is optimized for minimum footprint:

| Setting           | Value  | Effect                          |
|-------------------|--------|---------------------------------|
| `opt-level`       | `"z"`  | Optimize for size               |
| `lto`             | `true` | Link-time optimization          |
| `codegen-units`   | `1`    | Single codegen unit for max LTO |
| `overflow-checks` | `true` | Panic on integer overflow       |
| `strip`           | `true` | Strip debug symbols             |
| `panic`           | `abort`| No unwinding in WASM            |

---

## License

MIT
