use bip39::Mnemonic;
use ed25519_bip32::{XPrv, XPub, DerivationScheme, XPRV_SIZE};
use pallas_crypto::key::ed25519::PublicKey;
use pallas_traverse::ComputeHash;
use pallas_addresses::{Address, Network, ShelleyAddress, ShelleyPaymentPart, ShelleyDelegationPart};
use cryptoxide::{hmac::Hmac, pbkdf2::pbkdf2, sha2::Sha512};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic_str = std::env::args().nth(1).expect("Provide mnemonic as argument");
    let mnemonic = Mnemonic::parse(&mnemonic_str)?;

    // Step 1: Derive root key using PBKDF2 (ICARUS derivation)
    println!("Step 1: Derive root key from mnemonic");
    let entropy = mnemonic.to_entropy();
    println!("  Entropy: {}", hex::encode(&entropy));

    let mut pbkdf2_result = [0; XPRV_SIZE];
    const ITER: u32 = 4096;
    let mut mac = Hmac::new(Sha512::new(), "".as_bytes());
    pbkdf2(&mut mac, &entropy, ITER, &mut pbkdf2_result);
    let root_xprv = XPrv::normalize_bytes_force3rd(pbkdf2_result);
    println!("  Root XPrv (first 32 bytes): {}", hex::encode(&root_xprv.as_ref()[..32]));
    println!();

    // Step 2: Derive account key (m/1852'/1815'/0')
    println!("Step 2: Derive account key (m/1852'/1815'/0')");
    let purpose = root_xprv.derive(DerivationScheme::V2, 0x80000000 | 1852);
    let coin_type = purpose.derive(DerivationScheme::V2, 0x80000000 | 1815);
    let account_xprv = coin_type.derive(DerivationScheme::V2, 0x80000000 | 0);
    println!("  Account XPrv (first 32 bytes): {}", hex::encode(&account_xprv.as_ref()[..32]));

    // Step 3: Get account XPub
    println!();
    println!("Step 3: Get account XPub");
    let account_xpub: XPub = account_xprv.public();
    println!("  Account XPub (64 bytes): {}", hex::encode(account_xpub.as_ref()));
    println!("  - Public key (32 bytes):  {}", hex::encode(&account_xpub.as_ref()[..32]));
    println!("  - Chain code (32 bytes):  {}", hex::encode(&account_xpub.as_ref()[32..]));
    println!();

    // Step 4: Derive payment key (m/1852'/1815'/0'/0/0)
    println!("Step 4: Derive payment key from account XPub (0/0)");
    let payment_chain_xpub = account_xpub.derive(DerivationScheme::V2, 0)?;
    println!("  Payment chain XPub (first 32): {}", hex::encode(&payment_chain_xpub.as_ref()[..32]));

    let payment_xpub = payment_chain_xpub.derive(DerivationScheme::V2, 0)?;
    println!("  Payment XPub (64 bytes): {}", hex::encode(payment_xpub.as_ref()));

    let payment_pubkey_bytes = payment_xpub.public_key();
    println!("  Payment public key (32 bytes): {}", hex::encode(&payment_pubkey_bytes));
    println!();

    // Step 5: Hash payment key
    println!("Step 5: Hash payment key (Blake2b-224)");
    let payment_pubkey = PublicKey::from(payment_pubkey_bytes);
    let payment_hash = payment_pubkey.compute_hash();
    println!("  Payment key hash (28 bytes): {}", hex::encode(payment_hash.as_ref()));
    println!();

    // Step 6: Create enterprise address
    println!("Step 6: Create enterprise address (testnet)");
    let addr = ShelleyAddress::new(
        Network::Testnet,
        ShelleyPaymentPart::Key(payment_hash),
        ShelleyDelegationPart::Null,
    );
    let address_str = Address::Shelley(addr).to_bech32()?;
    println!("  Enterprise address: {}", address_str);
    println!();

    // Also test with hayate's current implementation for comparison
    println!("Comparison with hayate Wallet:");
    use hayate::wallet::{Wallet, Network as HayateNetwork};
    let wallet = Wallet::from_mnemonic_str(&mnemonic_str, HayateNetwork::Testnet, 0)?;

    // Debug: show what account keys Wallet has
    println!("  Wallet account_key (first 32): {}", hex::encode(&wallet.account().account_key.as_ref()[..32]));

    let hayate_addr = wallet.enterprise_address(0)?;
    println!("  Hayate enterprise address: {}", hayate_addr);
    println!("  Match: {}", address_str == hayate_addr);

    Ok(())
}
