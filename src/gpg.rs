// GPG encryption and decryption for wallet mnemonics
// Based on midnight-cli's GPG implementation

use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GpgError {
    #[error("GPG is not available. Please install gnupg.")]
    GpgNotAvailable,

    #[error("GPG encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("GPG decryption failed for file {file}: {error}")]
    DecryptionFailed {
        file: String,
        error: String,
    },

    #[error("Invalid UTF-8 in decrypted content: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type GpgResult<T> = Result<T, GpgError>;

/// GPG encryption and decryption support
pub struct Gpg;

impl Gpg {
    /// Check if GPG command is available
    pub fn is_available() -> bool {
        Command::new("gpg")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if a file is GPG encrypted
    #[allow(dead_code)]
    pub fn is_encrypted(path: &Path) -> bool {
        // Check file extension
        if path.extension().and_then(|s| s.to_str()) == Some("gpg") {
            return true;
        }

        // Check GPG magic bytes if file exists
        if let Ok(bytes) = std::fs::read(path) {
            Self::has_gpg_magic_bytes(&bytes)
        } else {
            false
        }
    }

    /// Check for GPG magic bytes
    #[allow(dead_code)]
    fn has_gpg_magic_bytes(bytes: &[u8]) -> bool {
        if bytes.len() < 2 {
            return false;
        }

        // PGP/GPG files typically start with specific magic bytes
        // OpenPGP message format: 0x85 (packet tag for compressed data)
        // or 0x8c (packet tag for marker packet)
        // or other packet types in range 0x80-0xBF
        matches!(bytes[0], 0x80..=0xBF) ||
        // ASCII armored format starts with "-----BEGIN PGP"
        bytes.starts_with(b"-----BEGIN PGP")
    }

    /// Encrypt a string with GPG using a recipient
    #[allow(dead_code)]
    pub fn encrypt_string(data: &str, recipient: &str) -> GpgResult<Vec<u8>> {
        if !Self::is_available() {
            return Err(GpgError::GpgNotAvailable);
        }

        let output = Command::new("gpg")
            .args(&[
                "--encrypt",
                "--armor",
                "--recipient",
                recipient,
                "--trust-model",
                "always",
                "--batch",
                "--yes",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?
            .stdin
            .ok_or_else(|| GpgError::EncryptionFailed("Failed to open stdin".to_string()))
            .and_then(|mut stdin| {
                use std::io::Write;
                stdin.write_all(data.as_bytes())?;
                Ok(())
            });

        if let Err(e) = output {
            return Err(GpgError::EncryptionFailed(e.to_string()));
        }

        let output = Command::new("gpg")
            .args(&[
                "--encrypt",
                "--armor",
                "--recipient",
                recipient,
                "--trust-model",
                "always",
                "--batch",
                "--yes",
            ])
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GpgError::EncryptionFailed(stderr.to_string()));
        }

        Ok(output.stdout)
    }

    /// Encrypt data to a file with GPG using a recipient
    pub fn encrypt_to_file(data: &str, recipient: &str, output_path: &Path) -> GpgResult<()> {
        if !Self::is_available() {
            return Err(GpgError::GpgNotAvailable);
        }

        let mut child = Command::new("gpg")
            .args(&[
                "--encrypt",
                "--armor",
                "--recipient",
                recipient,
                "--trust-model",
                "always",
                "--batch",
                "--yes",
                "--output",
            ])
            .arg(output_path)
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        {
            use std::io::Write;
            let stdin = child.stdin.as_mut()
                .ok_or_else(|| GpgError::EncryptionFailed("Failed to open stdin".to_string()))?;
            stdin.write_all(data.as_bytes())?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GpgError::EncryptionFailed(stderr.to_string()));
        }

        Ok(())
    }

    /// Decrypt a GPG encrypted file and return the contents
    pub fn decrypt_file(path: &Path) -> GpgResult<String> {
        if !Self::is_available() {
            return Err(GpgError::GpgNotAvailable);
        }

        let output = Command::new("gpg")
            .args(&["--decrypt", "--quiet", "--batch"])
            .arg(path)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GpgError::DecryptionFailed {
                file: path.display().to_string(),
                error: stderr.to_string(),
            });
        }

        let decrypted = String::from_utf8(output.stdout)?;
        Ok(decrypted)
    }

    /// Decrypt GPG encrypted bytes and return the contents
    #[allow(dead_code)]
    pub fn decrypt_bytes(data: &[u8]) -> GpgResult<String> {
        if !Self::is_available() {
            return Err(GpgError::GpgNotAvailable);
        }

        let mut child = Command::new("gpg")
            .args(&["--decrypt", "--quiet", "--batch"])
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        {
            use std::io::Write;
            let stdin = child.stdin.as_mut()
                .ok_or_else(|| GpgError::DecryptionFailed {
                    file: "<bytes>".to_string(),
                    error: "Failed to open stdin".to_string(),
                })?;
            stdin.write_all(data)?;
        }

        let output = child.wait_with_output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GpgError::DecryptionFailed {
                file: "<bytes>".to_string(),
                error: stderr.to_string(),
            });
        }

        let decrypted = String::from_utf8(output.stdout)?;
        Ok(decrypted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_encrypted_by_extension() {
        let path = PathBuf::from("test.txt.gpg");
        assert!(Gpg::is_encrypted(&path));

        let path = PathBuf::from("test.txt");
        assert!(!Gpg::is_encrypted(&path));
    }

    #[test]
    fn test_has_gpg_magic_bytes() {
        // ASCII armored PGP
        let armored = b"-----BEGIN PGP MESSAGE-----\nVersion: GnuPG";
        assert!(Gpg::has_gpg_magic_bytes(armored));

        // Binary PGP (packet tag byte)
        let binary = &[0x85, 0x01, 0x02, 0x03];
        assert!(Gpg::has_gpg_magic_bytes(binary));

        // Not PGP
        let plain = b"just plain text";
        assert!(!Gpg::has_gpg_magic_bytes(plain));

        // Empty
        assert!(!Gpg::has_gpg_magic_bytes(&[]));
    }

    #[test]
    fn test_gpg_availability() {
        // This test will pass/fail depending on whether GPG is installed
        // Just verify the function doesn't panic
        let _available = Gpg::is_available();
    }
}
