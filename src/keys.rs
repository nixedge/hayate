// Wallet key derivation and signing for Cardano
// Supports BIP39 mnemonics, CIP-1852 derivation paths, and CIP-8 message signing

#![allow(dead_code)]

use pallas_crypto::key::ed25519::{SecretKey, PublicKey, SecretKeyExtended};
use pallas_addresses::{Network, ShelleyAddress, ShelleyDelegationPart, ShelleyPaymentPart};
use pallas_traverse::ComputeHash;
use cryptoxide::{hmac::Hmac, pbkdf2::pbkdf2, sha2::Sha512};
use minicbor::{Encode, Encoder, encode};
use bip39::rand_core::OsRng;
use bip39::Mnemonic;
use ed25519_bip32::{XPrv, XPRV_SIZE};

/// Flexible secret key wrapper for both standard and extended keys
#[derive(Debug)]
pub enum FlexibleSecretKey {
    Standard(SecretKey),
    Extended(SecretKeyExtended),
}

/// Key pair with address
pub type KeyPairAndAddress = (FlexibleSecretKey, PublicKey, ShelleyAddress);

/// Generate a random Cardano key pair and address
pub fn generate_cardano_key_and_address() -> KeyPairAndAddress {
    let rng = OsRng;
    
    let sk = SecretKey::new(rng);
    let vk = sk.public_key();
    
    let addr = ShelleyAddress::new(
        Network::Mainnet,
        ShelleyPaymentPart::key_hash(vk.compute_hash()),
        ShelleyDelegationPart::Null,
    );
    
    (FlexibleSecretKey::Standard(sk), vk, addr)
}

/// Harden a BIP44 index
pub fn harden_index(index: u32) -> u32 {
    index | 0x80000000 // Set MSB
}

/// Derive key pair from BIP39 mnemonic using CIP-1852 derivation
/// Path: m/1852'/1815'/account'/0/index (payment key only)
pub fn derive_key_pair_from_mnemonic(
    mnemonic: &str,
    account: u32,
    index: u32,
) -> KeyPairAndAddress {
    let bip39 = Mnemonic::parse(mnemonic).expect("Valid mnemonic required");
    let entropy = bip39.to_entropy();
    
    // PBKDF2 to derive root key
    let mut pbkdf2_result = [0; XPRV_SIZE];
    const ITER: u32 = 4096;
    let mut mac = Hmac::new(Sha512::new(), "".as_bytes());
    pbkdf2(&mut mac, &entropy, ITER, &mut pbkdf2_result);
    let xprv = XPrv::normalize_bytes_force3rd(pbkdf2_result);
    
    // CIP-1852 derivation: m/1852'/1815'/account'/0/index
    let pay_xprv = &xprv
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1852))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1815))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(account))
        .derive(ed25519_bip32::DerivationScheme::V2, 0) // External chain
        .derive(ed25519_bip32::DerivationScheme::V2, index)
        .extended_secret_key();
    
    unsafe {
        let sk = SecretKeyExtended::from_bytes_unchecked(*pay_xprv);
        let vk = sk.public_key();
        
        let addr = ShelleyAddress::new(
            Network::Mainnet,
            ShelleyPaymentPart::key_hash(vk.compute_hash()),
            ShelleyDelegationPart::Null,
        );
        
        (FlexibleSecretKey::Extended(sk), vk, addr)
    }
}

/// Derive key pair with both payment and stake keys
/// Payment: m/1852'/1815'/account'/0/index
/// Stake: m/1852'/1815'/account'/2/index
pub fn derive_key_pair_with_stake(
    mnemonic: &str,
    account: u32,
    index: u32,
) -> KeyPairAndAddress {
    let bip39 = Mnemonic::parse(mnemonic).expect("Valid mnemonic required");
    let entropy = bip39.to_entropy();
    
    let mut pbkdf2_result = [0; XPRV_SIZE];
    const ITER: u32 = 4096;
    let mut mac = Hmac::new(Sha512::new(), "".as_bytes());
    pbkdf2(&mut mac, &entropy, ITER, &mut pbkdf2_result);
    let xprv = XPrv::normalize_bytes_force3rd(pbkdf2_result);
    
    // Derive payment key
    let pay_xprv = &xprv
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1852))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1815))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(account))
        .derive(ed25519_bip32::DerivationScheme::V2, 0)
        .derive(ed25519_bip32::DerivationScheme::V2, index)
        .extended_secret_key();
    
    // Derive stake key
    let stake_xprv = &xprv
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1852))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(1815))
        .derive(ed25519_bip32::DerivationScheme::V2, harden_index(account))
        .derive(ed25519_bip32::DerivationScheme::V2, 2) // Staking chain
        .derive(ed25519_bip32::DerivationScheme::V2, index)
        .extended_secret_key();
    
    unsafe {
        let pay_priv = SecretKeyExtended::from_bytes_unchecked(*pay_xprv);
        let pay_pub = pay_priv.public_key();
        let stake_pub = SecretKeyExtended::from_bytes_unchecked(*stake_xprv).public_key();
        
        let addr = ShelleyAddress::new(
            Network::Mainnet,
            ShelleyPaymentPart::key_hash(pay_pub.compute_hash()),
            ShelleyDelegationPart::key_hash(stake_pub.compute_hash()),
        );
        
        (FlexibleSecretKey::Extended(pay_priv), pay_pub, addr)
    }
}

/// Generate key pair from hex-encoded secret key
pub fn generate_key_pair_from_hex(sk_hex: &str) -> KeyPairAndAddress {
    let skey_bytes = hex::decode(sk_hex).expect("Invalid secret key hex");
    let skey_array: [u8; 32] = skey_bytes
        .try_into()
        .expect("Secret key must be exactly 32 bytes");
    let sk = SecretKey::from(skey_array);
    let vk = sk.public_key();
    
    let addr = ShelleyAddress::new(
        Network::Mainnet,
        ShelleyPaymentPart::key_hash(vk.compute_hash()),
        ShelleyDelegationPart::Null,
    );
    
    (FlexibleSecretKey::Standard(sk), vk, addr)
}

// ===== CIP-8 Message Signing =====

#[derive(Debug)]
struct CoseProtHeader {
    address: Vec<u8>,
}

impl<C> Encode<C> for CoseProtHeader
where
    C: Default,
{
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> Result<(), encode::Error<W::Error>> {
        e.map(2)?;
        e.i64(1)?;
        e.i64(-8)?;
        e.str("address")?;
        e.bytes(&self.address)?;
        Ok(())
    }
}

#[derive(Debug)]
struct CoseSignData<'a> {
    label: &'a str,
    protected_header: &'a [u8],
    external_aad: &'a [u8],
    payload: &'a [u8],
}

impl<C> Encode<C> for CoseSignData<'_>
where
    C: Default,
{
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> Result<(), encode::Error<W::Error>> {
        e.array(4)?;
        e.str(self.label)?;
        e.bytes(self.protected_header)?;
        e.bytes(self.external_aad)?;
        e.bytes(self.payload)?;
        Ok(())
    }
}

#[derive(Debug)]
struct CoseSign1<'a> {
    protected_header: &'a [u8],
    payload: &'a [u8],
    signature: &'a [u8],
}

impl<C> Encode<C> for CoseSign1<'_>
where
    C: Default,
{
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut C,
    ) -> Result<(), encode::Error<W::Error>> {
        e.array(4)?;
        e.bytes(self.protected_header)?;
        e.map(1)?;
        e.str("hashed")?;
        e.bool(false)?;
        e.bytes(self.payload)?;
        e.bytes(self.signature)?;
        Ok(())
    }
}

/// Sign a message using CIP-8 format
/// Returns (signed_message_hex, public_key_hex)
pub fn cip8_sign(kp: &KeyPairAndAddress, message: &str) -> (String, String) {
    let pubkey = hex::encode(kp.1.as_ref());
    
    let prot_header = CoseProtHeader {
        address: kp.2.to_vec(),
    };
    let cose_prot_cbor = minicbor::to_vec(&prot_header).unwrap();
    
    let to_sign = CoseSignData {
        label: "Signature1",
        protected_header: &cose_prot_cbor,
        external_aad: b"",
        payload: message.as_bytes(),
    };
    let to_sign_cbor = minicbor::to_vec(&to_sign).unwrap();
    
    let signature = match &kp.0 {
        FlexibleSecretKey::Standard(sk) => sk.sign(&to_sign_cbor),
        FlexibleSecretKey::Extended(ske) => ske.sign(&to_sign_cbor),
    };
    
    let cose_struct = CoseSign1 {
        protected_header: &cose_prot_cbor,
        payload: message.as_bytes(),
        signature: signature.as_ref(),
    };
    let cose_sign1_cbor = minicbor::to_vec(&cose_struct).unwrap();

    (hex::encode(&cose_sign1_cbor), pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_random_key() {
        let (sk, _pk, _addr) = generate_cardano_key_and_address();
        // Should generate valid key pair and address
        assert!(matches!(sk, FlexibleSecretKey::Standard(_)));
    }
    
    #[test]
    fn test_harden_index() {
        assert_eq!(harden_index(0), 0x80000000);
        assert_eq!(harden_index(1), 0x80000001);
    }
    
    #[test]
    fn test_cip8_signing() {
        let kp = generate_cardano_key_and_address();
        let (signed_msg, pubkey) = cip8_sign(&kp, "test message");
        
        // Should produce hex-encoded signed message and pubkey
        assert!(!signed_msg.is_empty());
        assert!(!pubkey.is_empty());
        assert_eq!(pubkey.len(), 64); // 32 bytes = 64 hex chars
    }
}
