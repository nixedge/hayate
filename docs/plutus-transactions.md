# Plutus Transaction Support

Hayate now includes full native support for building Plutus script transactions on Cardano (Conway era). This eliminates the need for external dependencies like `cardano-cli` for contract deployment and script interactions.

## Architecture

The Plutus transaction support is built on three main components:

### 1. Plutus Module (`src/wallet/plutus/`)

Core types and utilities for Plutus scripts:

- **`script.rs`**: `PlutusScript` wrapper with version support (V1, V2, V3)
- **`address.rs`**: Script address calculation using BLAKE2b-224
- **`datum.rs`**: Datum construction including `VersionedMultisig` for governance
- **`redeemer.rs`**: Redeemer builders with execution units
- **`cost_models.rs`**: Standard Plutus cost models for all versions

### 2. Transaction Builder (`src/wallet/tx_builder/`)

High-level API wrapping `pallas-txbuilder`:

- **`builder.rs`**: `PlutusTransactionBuilder` for Conway-era transactions
- **`types.rs`**: `PlutusInput` and `PlutusOutput` types
- Full support for:
  - Script inputs with redeemers
  - Inline datums and datum hashes
  - Script references
  - Collateral inputs
  - Multi-asset outputs
  - Language views (cost models)

### 3. Integration

Uses `pallas-txbuilder` 0.35.0 for correct CBOR encoding and script data hash calculation.

## Usage

### Basic Contract Deployment

```rust
use hayate::wallet::plutus::*;
use hayate::wallet::tx_builder::*;

// 1. Load your Plutus script
let script = PlutusScript::v2_from_cbor(script_bytes)?;

// 2. Create a datum
let datum = VersionedMultisig {
    threshold: 2,
    members: vec![/* ... */],
    logic_round: 0,
};

// 3. Build transaction
let mut builder = PlutusTransactionBuilder::new(
    Network::Testnet,
    change_address
);

// Add funding input
builder.add_input(&PlutusInput::regular(utxo))?;

// Add output with script and datum
let output = PlutusOutput::new(script.address(Network::Testnet)?, 50_000_000)
    .with_datum(DatumOption::inline(datum.to_cbor()?))
    .with_script_ref(script.clone());

builder.add_output(&output)?;

// Add collateral and parameters
builder.add_collateral(&collateral_utxo)?;
builder
    .set_fee(200_000)
    .set_ttl(current_slot + 1000)
    .set_network_id()
    .set_default_language_view(PlutusVersion::V2);

// Build
let (tx_bytes, tx_hash) = builder.build()?;
```

### Spending from a Script

```rust
// 1. Create redeemer for spending
let redeemer = Redeemer::spend(0, redeemer_data);

// 2. Add script input
let script_input = PlutusInput::script(
    utxo,
    script,
    redeemer,
    None  // datum is inline in the UTxO
);

builder.add_input(&script_input)?;

// 3. Add language view (REQUIRED for script execution)
builder.set_default_language_view(PlutusVersion::V2);

// Rest is same as above
```

### Governance Contract Datum

The `VersionedMultisig` datum is used for governance contracts in Midnight:

```rust
let datum = VersionedMultisig {
    threshold: 2,  // Requires 2 signatures
    members: vec![
        GovernanceMember {
            cardano_hash: cardano_key_hash,  // 28 bytes
            sr25519_key: sr25519_pubkey,     // 32 bytes
        },
        // ... more members
    ],
    logic_round: 0,  // Governance round
};

let datum_cbor = datum.to_cbor()?;
let datum_hash = plutus::datum_hash(&datum_cbor);
```

## Example

See `examples/deploy_plutus_contract.rs` for a complete working example of contract deployment.

Run it with:
```bash
cargo run --example deploy_plutus_contract
```

## Cost Models

Default cost models for each Plutus version are provided:

```rust
use hayate::wallet::plutus::default_cost_model;

let v2_costs = default_cost_model(PlutusVersion::V2);
builder.set_language_view(PlutusVersion::V2, v2_costs);

// Or use the convenience method:
builder.set_default_language_view(PlutusVersion::V2);
```

These are the mainnet cost models as of Conway era.

## Integration with Midnight Network Setup

When deploying a new Midnight network, the Plutus transaction support is used to:

1. **Deploy governance contracts** to Cardano testnet (SanchoNet)
2. **Lock initial funds** in the multisig contract
3. **Generate UTxO references** for the genesis configuration

### Workflow:

```bash
# 1. Generate validator keys (midnight-cli)
midnight-cli validator generate --count 3 --output validator-keys.json

# 2. Deploy contract using hayate's PlutusTransactionBuilder
# (Programmatically or via integration in midnight-cli)

# 3. Include UTxO reference in genesis config
midnight-cli genesis init \\
    --validators validator-keys.json \\
    --cardano-contract <tx_hash>#<output_index>
```

## Implementation Notes

### Why pallas-txbuilder?

- **Correct CBOR encoding**: Ensures transactions match Cardano's exact format
- **Script data hash**: Properly calculates script data hash from redeemers, datums, and cost models
- **Conway era support**: Native support for latest transaction format
- **Battle-tested**: Used by TxPipe and other production tools

### Key Design Decisions

1. **Wrapper approach**: High-level API wrapping `pallas-txbuilder::StagingTransaction`
2. **No manual CBOR**: All encoding delegated to pallas
3. **Type safety**: Strong typing for inputs, outputs, scripts, and datums
4. **Network awareness**: Testnet/Mainnet address generation
5. **Cost model helpers**: Built-in standard cost models

## Testing

Run the test suite:

```bash
# All tests
cargo test --lib

# Plutus module only
cargo test --lib plutus

# Transaction builder only
cargo test --lib tx_builder
```

Current test coverage: 113 tests passing
- 38 tests for plutus module
- 14 tests for tx_builder
- 6 tests for cost models

## Dependencies

- `pallas-txbuilder` 0.35.0 - Transaction building
- `pallas-addresses` 0.35.0 - Address parsing/generation
- `pallas-crypto` 0.35.0 - Hashing (BLAKE2b)
- `pallas-wallet` 0.35.0 - Key management and signing
- `pallas-primitives` 0.35.0 - Cardano types
- `pallas-codec` 0.35.0 - CBOR encoding

## Future Work

- [ ] Fee estimation based on transaction size and ExUnits
- [ ] UTxO selection and coin selection algorithms
- [ ] Metadata support
- [ ] Certificate support (stake registration, delegation)
- [ ] Withdrawal support (rewards)
- [ ] Reference input support
- [ ] Voting procedures (Conway governance)

## References

- [Cardano Developer Portal - Plutus](https://developers.cardano.org/docs/smart-contracts/plutus/)
- [CIP-0057: Plutus Core v3](https://cips.cardano.org/cip/CIP-0057)
- [pallas Documentation](https://github.com/txpipe/pallas)
- [Conway Ledger Spec](https://github.com/input-output-hk/cardano-ledger)
