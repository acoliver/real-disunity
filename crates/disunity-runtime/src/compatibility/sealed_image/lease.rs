use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, MetadataExt};

use rustix::fs::{OFlags, SealFlags, fcntl_get_seals, fcntl_getfl};
use rustix::io::{FdFlags, fcntl_getfd};
use sha2::{Digest, Sha256};

use super::rejected;
use crate::HostError;

const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Owns verified sealed bytes in this process. The expected digest identifies
/// bytes selected by the host; it grants no code, provider or project authority.
/// This Linux profile deliberately does not transfer descriptors across exec.
/// There is no pathname constructor or public descriptor accessor.
///
/// ```compile_fail
/// use worldrunner_disunity_runtime::NativeImageLease;
/// fn duplicate(image: &NativeImageLease) -> NativeImageLease { image.clone() }
/// ```
///
/// ```compile_fail
/// use worldrunner_disunity_runtime::NativeImageLease;
/// let image: NativeImageLease = serde_json::from_str("{}").unwrap();
/// ```
///
/// ```compile_fail
/// use worldrunner_disunity_runtime::NativeImageLease;
/// let image = NativeImageLease::from_file("/proc/self/fd/3", &"0".repeat(64));
/// ```
pub struct NativeImageLease {
    file: File,
    digest: String,
    identity: (u64, u64, u64),
}

impl NativeImageLease {
    /// Consumes a read-only regular descriptor with all four irreversible seals.
    /// An atomic CLOEXEC duplicate is owned before validation or positional reads.
    pub fn from_file(file: File, expected_sha256: &str) -> Result<Self, HostError> {
        if expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(rejected(
                "expected SHA-256 must be 64 lowercase hex characters",
            ));
        }
        let file = File::from(rustix::io::fcntl_dupfd_cloexec(&file, 0).map_err(rejected)?);
        let metadata = file.metadata().map_err(rejected)?;
        let len = metadata.len();
        if !metadata.is_file() || len == 0 || len > MAX_BYTES {
            return Err(rejected(
                "requires a nonempty regular file of at most 64 MiB",
            ));
        }
        if (fcntl_getfl(&file).map_err(rejected)? & OFlags::ACCMODE) != OFlags::RDONLY
            || !fcntl_getfd(&file)
                .map_err(rejected)?
                .contains(FdFlags::CLOEXEC)
        {
            return Err(rejected("requires read-only CLOEXEC ownership"));
        }
        let required = SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL;
        if !fcntl_get_seals(&file).map_err(rejected)?.contains(required) {
            return Err(rejected("requires effective WRITE/GROW/SHRINK/SEAL seals"));
        }
        if hash(&file, len)? != expected_sha256 {
            return Err(rejected("host-selected SHA-256 mismatch"));
        }
        Ok(Self {
            file,
            digest: expected_sha256.into(),
            identity: (metadata.dev(), metadata.ino(), len),
        })
    }

    pub fn sha256(&self) -> &str {
        &self.digest
    }
    pub fn len(&self) -> u64 {
        self.identity.2
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn same_image(&self, other: &Self) -> bool {
        self.identity == other.identity && self.digest == other.digest
    }

    pub(super) fn load_path(&self) -> std::path::PathBuf {
        format!("/proc/self/fd/{}", self.file.as_raw_fd()).into()
    }
}

fn hash(file: &File, len: u64) -> Result<String, HostError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut offset = 0;
    while offset < len {
        let count = file.read_at(&mut buffer, offset).map_err(rejected)?;
        if count == 0 || count as u64 > len - offset {
            return Err(rejected("sealed length changed during verification"));
        }
        hasher.update(&buffer[..count]);
        offset += count as u64;
    }
    if file.read_at(&mut buffer[..1], len).map_err(rejected)? != 0 {
        return Err(rejected("sealed file exceeds verified length"));
    }
    Ok(format!("{:x}", hasher.finalize()))
}
