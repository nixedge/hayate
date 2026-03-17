// BIP39 mnemonic generation and validation

use bip39::{Language, Mnemonic};
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum MnemonicError {
    #[error("Failed to generate mnemonic: {0}")]
    GenerationFailed(String),

    #[error("Invalid mnemonic: {0}")]
    InvalidMnemonic(String),

    #[error("Invalid word count: expected 12, 15, 18, 21, or 24 words, got {0}")]
    InvalidWordCount(usize),
}

pub type MnemonicResult<T> = Result<T, MnemonicError>;

/// Generate a new random BIP39 mnemonic phrase
pub fn generate_mnemonic(word_count: usize) -> MnemonicResult<String> {
    use rand::Rng;

    // Map word count to entropy bytes
    // 12 words = 16 bytes, 15 = 20, 18 = 24, 21 = 28, 24 = 32
    let entropy_bytes = match word_count {
        12 => 16,
        15 => 20,
        18 => 24,
        21 => 28,
        24 => 32,
        _ => return Err(MnemonicError::InvalidWordCount(word_count)),
    };

    // Generate random entropy
    let mut rng = rand::thread_rng();
    let mut entropy = vec![0u8; entropy_bytes];
    rng.fill(&mut entropy[..]);

    // Create mnemonic from entropy
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|e| MnemonicError::GenerationFailed(e.to_string()))?;

    Ok(mnemonic.to_string())
}

/// Validate a mnemonic phrase
pub fn validate_mnemonic(phrase: &str) -> MnemonicResult<()> {
    Mnemonic::parse_in(Language::English, phrase)
        .map_err(|e| MnemonicError::InvalidMnemonic(e.to_string()))?;
    Ok(())
}

/// Normalize a mnemonic phrase (trim whitespace, lowercase, etc.)
pub fn normalize_mnemonic(phrase: &str) -> String {
    phrase
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Parse a mnemonic from a string
pub fn parse_mnemonic(phrase: &str) -> MnemonicResult<Mnemonic> {
    let normalized = normalize_mnemonic(phrase);
    Mnemonic::parse_in(Language::English, &normalized)
        .map_err(|e| MnemonicError::InvalidMnemonic(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str =
        "bottom drive obey lake curtain smoke basket hold race lonely fit walk";

    #[test]
    fn test_generate_mnemonic_24_words() {
        let mnemonic = generate_mnemonic(24).unwrap();
        assert_eq!(mnemonic.split_whitespace().count(), 24);
        assert!(validate_mnemonic(&mnemonic).is_ok());
    }

    #[test]
    fn test_generate_mnemonic_12_words() {
        let mnemonic = generate_mnemonic(12).unwrap();
        assert_eq!(mnemonic.split_whitespace().count(), 12);
        assert!(validate_mnemonic(&mnemonic).is_ok());
    }

    #[test]
    fn test_validate_mnemonic() {
        assert!(validate_mnemonic(TEST_MNEMONIC).is_ok());
        assert!(validate_mnemonic("invalid mnemonic phrase").is_err());
        assert!(validate_mnemonic("").is_err());
    }

    #[test]
    fn test_normalize_mnemonic() {
        let messy = "  bottom   drive  obey\nlake  curtain   smoke  ";
        let normalized = normalize_mnemonic(messy);
        assert_eq!(normalized, "bottom drive obey lake curtain smoke");

        let with_caps = "Bottom DRIVE Obey";
        let normalized = normalize_mnemonic(with_caps);
        assert_eq!(normalized, "bottom drive obey");
    }
}
