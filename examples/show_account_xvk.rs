// Show account xvk for a mnemonic

use bip39::Mnemonic;
use ed25519_bip32::{XPrv, DerivationScheme, XPRV_SIZE};
use cryptoxide::{hmac::Hmac, pbkdf2::pbkdf2, sha2::Sha512};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mnemonic_str = std::env::args().nth(1).expect("Provide mnemonic as argument");
    let mnemonic = Mnemonic::parse(&mnemonic_str)?;

    // Derive root key using PBKDF2 (ICARUS)
    let entropy = mnemonic.to_entropy();
    let mut pbkdf2_result = [0; XPRV_SIZE];
    const ITER: u32 = 4096;
    let mut mac = Hmac::new(Sha512::new(), "".as_bytes());
    pbkdf2(&mut mac, &entropy, ITER, &mut pbkdf2_result);
    let root_xprv = XPrv::normalize_bytes_force3rd(pbkdf2_result);

    // Derive account key: m/1852'/1815'/0'
    let purpose = root_xprv.derive(DerivationScheme::V2, 0x80000000 | 1852);
    let coin_type = purpose.derive(DerivationScheme::V2, 0x80000000 | 1815);
    let account_xprv = coin_type.derive(DerivationScheme::V2, 0x80000000 | 0);
    let account_xpub = account_xprv.public();

    // Encode as bech32
    let xpub_bytes = account_xpub.as_ref();
    let hrp = bech32::Hrp::parse("acct_xvk").unwrap();
    let bech32_str = bech32::encode::<bech32::Bech32>(hrp, xpub_bytes)?;

    println!("Account XPub (hex):    {}", hex::encode(xpub_bytes));
    println!("Account XPub (bech32): {}", bech32_str);

    Ok(())
}
