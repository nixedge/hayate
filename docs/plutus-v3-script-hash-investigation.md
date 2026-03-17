# Plutus V3 Script Hash Investigation

## Problem Statement

After upgrading hayate to pallas 1.0.0-alpha.5 and uplc 1.1.21, Plutus V3 script evaluation in the simulator was failing with errors like:
- `FlatDecode: unexpected type u8 at position 0: expected bytes (definite length)`
- `MissingRequiredScript` with hash `629b415ec93feef820ac7acff484d194cad199ac2930aec9f4b4150e`

Even though a reference transaction from SanchoNet (built with cardano-cli) worked correctly on the actual network.

## Investigation Process

### Initial Hypothesis: Pallas 1.0 vs uplc Incompatibility

Initially suspected that pallas 1.0 transaction encoding was incompatible with uplc 1.1.21 (which uses pallas 0.33 internally).

**Evidence that disproved this:**
- Pallas 1.0 can decode and re-encode the SanchoNet transaction byte-for-byte identically
- The re-encoded transaction was accepted by the SanchoNet network
- Both original and pallas-re-encoded transactions behaved identically in tests

### Second Hypothesis: Script Hash Computation Error

Noticed two different hash values in testing:
- Hash A: `629b415ec93feef820ac7acff484d194cad199ac2930aec9f4b4150e` (hash of raw script bytes)
- Hash B: `d27ccc13fab5b782984a3d1f99353197ca1a81be069941ffc003ee75` (hash of Aiken CBOR)

The raw script bytes for always_true validator:
```
01010029800aba2aba1aab9eaab9dab9a4888896600264653001300600198031803800cc0180092225980099b8748008c01cdd500144c8cc892898050009805180580098041baa0028b200c180300098019baa0068a4d13656400401
```

The Aiken CBOR format (with `585c` byte string wrapper):
```
585c01010029800aba2aba1aab9eaab9dab9a4888896600264653001300600198031803800cc0180092225980099b8748008c01cdd500144c8cc892898050009805180580098041baa0028b200c180300098019baa0068a4d13656400401
```

Initially assumed Hash A (raw bytes) was correct, but this led to `MissingRequiredScript` errors.

### Key Discoveries

1. **CBOR Encoding in Witness Set:**
   - Working SanchoNet transaction has double-wrapped scripts: `585e585c<script>`
   - Outer wrapper (`585e`) added by pallas when encoding PlutusScript
   - Inner wrapper (`585c`) is the Aiken CBOR format

2. **Script Hash Computation:**
   - When using Hash A (raw bytes) in ledger state: transactions fail with `MissingRequiredScript`
   - When using Hash B (Aiken CBOR) in ledger state: transactions work correctly

3. **Verification with cardano-cli:**
   ```bash
   # Using Aiken CBOR format
   echo '{"type": "PlutusScriptV3", "description": "", "cborHex": "585c0101..."}' > script.plutus
   cardano-cli conway address build --payment-script-file script.plutus --testnet-magic 4
   ```

   Output: `addr_test1wrf8enqnl26m0q5cfg73lxf4xxtu5x5phcrfjs0lcqp7uagh2hm3k`

   Address breakdown: `70d27ccc13fab5b782984a3d1f99353197ca1a81be069941ffc003ee75`
   - `70` = network/address type byte
   - `d27ccc...` = **script hash (Hash B - hash of Aiken CBOR)**

## Resolution

**The Correct Approach:**

Plutus V3 script hashes are computed from the **CBOR-encoded script** (with byte string wrapper), NOT from the raw flat-encoded UPLC bytes.

### Hash Computation Formula

```
script_hash = blake2b_224(tag_byte || cbor_encoded_script)
```

Where:
- `tag_byte` = `0x03` (for Plutus V3)
- `cbor_encoded_script` = the Aiken CBOR format (e.g., `585c<flat_script>`)

### Why This Makes Sense

1. **Consistent with Cardano Standards:** The CBOR encoding is what gets stored in witness sets and transmitted on the network
2. **Version Flexibility:** The CBOR wrapper includes metadata about the script format
3. **Tooling Compatibility:** cardano-cli and other standard tools use this approach

## Implementation in Hayate

### Building Transactions with Plutus Scripts

```rust
// Decode Aiken CBOR to get raw script bytes for validation
let aiken_cbor_bytes = hex::decode(script_cbor)?;
let script_bytes_raw: ByteVec = decode(&aiken_cbor_bytes)?;

// Verify script parses correctly
match Program::<FakeNamedDeBruijn>::from_flat(&script_bytes_raw) {
    Ok(program) => { /* script is valid */ }
    Err(e) => return Err(anyhow!("Invalid script: {:?}", e)),
}

// For witness set: use Aiken CBOR (pallas will add outer wrapper)
use pallas_primitives::Bytes;
let script_for_witness: Bytes = aiken_cbor_bytes.into();

// For script hash: hash the Aiken CBOR
let script_hash = Hasher::<224>::hash_tagged(&aiken_cbor_bytes, 3);
```

### Ledger State Format

UTxO outputs with script addresses should use the hash of the Aiken CBOR:

```json
{
  "utxos": {
    "txhash:index": "a300581d70d27ccc13fab5b782984a3d1f99353197ca1a81be069941ffc003ee75011a00989680028201d8184100"
  }
}
```

Where `70d27ccc...` = `70` (network byte) + `d27ccc...` (hash of Aiken CBOR).

## Lessons Learned

1. **Always verify assumptions against reference implementations** (cardano-cli, cardano-node)
2. **CBOR encoding matters** - the format stored on-chain is what gets hashed, not the underlying data
3. **Test against actual network behavior** - the SanchoNet transaction submission was the ultimate validation
4. **Don't assume bugs in mature systems** - Cardano's script hashing is well-tested and correct

## Related Files

- Test validators: `/home/sam/work/iohk/hayate/tests/simulator_test.rs`
- Ledger state: `/home/sam/work/iohk/hayate/test-ledger-state.json`
- Script constants: `ALWAYS_TRUE_CBOR`, `CHECK_42_CBOR`, `MATCH_DATUM_CBOR` (Aiken CBOR format)

## Verification

To verify script hash computation:

```bash
# 1. Create script file with Aiken CBOR
echo '{"type": "PlutusScriptV3", "description": "", "cborHex": "<aiken_cbor>"}' > script.plutus

# 2. Get address (contains script hash)
cardano-cli conway address build --payment-script-file script.plutus --testnet-magic 4

# 3. Decode address to see script hash
cardano-cli conway address info --address <address>
```

The `base16` field will show: `70<script_hash>` (for testnet script addresses).
