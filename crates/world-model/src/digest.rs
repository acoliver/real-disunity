//! `SHA-256` digest wrappers used for content identity and state digests.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// Byte length of a `SHA-256` digest.
pub const SHA256_LEN: usize = 32;

const HEX_LEN: usize = SHA256_LEN * 2;
const ZERO: [u8; SHA256_LEN] = [0; SHA256_LEN];

/// Failures that can occur when parsing or constructing digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DigestError {
    /// The hex string did not contain exactly 64 characters.
    #[error("digest hex must be exactly 64 hexadecimal characters")]
    InvalidHexLength,
    /// The hex string contained a non-hexadecimal character.
    #[error("digest hex contains a non-hexadecimal character")]
    InvalidHexCharacter,
    /// The all-zero digest is reserved and never a valid content identity.
    #[error("all-zero digest is not a valid content identity")]
    ZeroDigest,
}

/// Nonzero `SHA-256` content digest.
///
/// This wrapper is the crate's only content-identity spelling: bytes are
/// private, parsing is strict, and the all-zero digest is rejected because it
/// is reserved for "missing content". Zero-byte content is fine; its digest is
/// simply the ordinary hash of the empty byte string, which is nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentDigest([u8; SHA256_LEN]);

impl ContentDigest {
    /// Creates a digest from exact bytes, rejecting the all-zero digest.
    pub fn from_bytes(bytes: [u8; SHA256_LEN]) -> Result<Self, DigestError> {
        if bytes == ZERO {
            return Err(DigestError::ZeroDigest);
        }
        Ok(Self(bytes))
    }

    /// Parses a strict 64-character hex digest.
    ///
    /// Lowercase is canonical, but uppercase hex is accepted on input. Malformed
    /// input and the all-zero digest are rejected.
    pub fn from_hex(hex: &str) -> Result<Self, DigestError> {
        Self::from_bytes(decode_hex(hex)?)
    }

    /// Returns the raw digest bytes.
    pub fn as_bytes(&self) -> &[u8; SHA256_LEN] {
        &self.0
    }

    /// Returns the canonical lowercase hex spelling.
    pub fn to_hex(&self) -> String {
        encode_hex(&self.0)
    }

    /// Wraps bytes produced by this crate's own hashing.
    ///
    /// Trusted invariants: canonical hashing only emits `SHA-256` outputs, and
    /// externally supplied identities must use the strict public constructors.
    pub(crate) fn from_computed(bytes: [u8; SHA256_LEN]) -> Self {
        Self(bytes)
    }

    fn into_inner(self) -> [u8; SHA256_LEN] {
        self.0
    }
}

/// Digest of a fully validated world document combined with the caller's
/// corpus-manifest and policy digests.
///
/// This is a distinct domain artifact produced only by
/// [`crate::SemanticWorldDocument::generated_state_digest`]. Parsing stays
/// strict so downstream users can round-trip it safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeneratedStateDigest([u8; SHA256_LEN]);

impl GeneratedStateDigest {
    /// Creates the wrapper from exact bytes, rejecting the all-zero digest.
    pub fn from_bytes(bytes: [u8; SHA256_LEN]) -> Result<Self, DigestError> {
        if bytes == ZERO {
            return Err(DigestError::ZeroDigest);
        }
        Ok(Self(bytes))
    }

    /// Parses a strict 64-character hex digest (lowercase or uppercase).
    pub fn from_hex(hex: &str) -> Result<Self, DigestError> {
        Ok(Self(ContentDigest::from_hex(hex)?.into_inner()))
    }

    /// Returns the raw digest bytes.
    pub fn as_bytes(&self) -> &[u8; SHA256_LEN] {
        &self.0
    }

    /// Returns the canonical lowercase hex spelling.
    pub fn to_hex(&self) -> String {
        encode_hex(&self.0)
    }

    pub(crate) fn from_computed(bytes: [u8; SHA256_LEN]) -> Self {
        Self(bytes)
    }
}

/// Finalizes a `SHA-256` hasher into a computed content digest.
pub(crate) fn finalize_digest(hasher: Sha256) -> ContentDigest {
    let bytes: [u8; SHA256_LEN] = hasher.finalize().into();
    ContentDigest::from_computed(bytes)
}

fn encode_hex(bytes: &[u8; SHA256_LEN]) -> String {
    const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(HEX_LEN);
    for byte in bytes {
        let upper = usize::from(*byte >> 4);
        let lower = usize::from(*byte & 0x0f);
        out.push(char::from(HEX_DIGITS[upper]));
        out.push(char::from(HEX_DIGITS[lower]));
    }
    out
}

fn decode_hex(hex: &str) -> Result<[u8; SHA256_LEN], DigestError> {
    let bytes = hex.as_bytes();
    if bytes.len() != HEX_LEN {
        return Err(DigestError::InvalidHexLength);
    }
    let mut digest = [0; SHA256_LEN];
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        let hi = hex_digit_value(pair[0])?;
        let lo = hex_digit_value(pair[1])?;
        digest[index] = (hi << 4) | lo;
    }
    Ok(digest)
}

fn hex_digit_value(byte: u8) -> Result<u8, DigestError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(DigestError::InvalidHexCharacter),
    }
}

impl Serialize for ContentDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        ContentDigest::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

impl Serialize for GeneratedStateDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for GeneratedStateDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        GeneratedStateDigest::from_hex(&hex).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_uses_lowercase_canonical_form() {
        let digest = ContentDigest::from_bytes([1; SHA256_LEN]).expect("nonzero digest");
        let hex = digest.to_hex();
        assert_eq!(hex.len(), HEX_LEN);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        let parsed = ContentDigest::from_hex(&hex).expect("canonical hex parses");
        assert_eq!(parsed, digest);
    }

    #[test]
    fn uppercase_hex_is_accepted() {
        let digest = ContentDigest::from_bytes([0xab; SHA256_LEN]).expect("nonzero digest");
        let upper: String = digest.to_hex().to_uppercase();
        assert_eq!(ContentDigest::from_hex(&upper), Ok(digest));
    }

    #[test]
    fn malformed_hex_is_rejected() {
        assert_eq!(
            ContentDigest::from_hex("ab"),
            Err(DigestError::InvalidHexLength)
        );
        assert_eq!(
            ContentDigest::from_hex(&"z".repeat(HEX_LEN)),
            Err(DigestError::InvalidHexCharacter)
        );
    }

    #[test]
    fn all_zero_digest_is_rejected() {
        assert_eq!(
            ContentDigest::from_bytes(ZERO),
            Err(DigestError::ZeroDigest)
        );
        assert_eq!(
            ContentDigest::from_hex(&"0".repeat(HEX_LEN)),
            Err(DigestError::ZeroDigest)
        );
        assert_eq!(
            GeneratedStateDigest::from_hex(&"0".repeat(HEX_LEN)),
            Err(DigestError::ZeroDigest)
        );
    }
}
