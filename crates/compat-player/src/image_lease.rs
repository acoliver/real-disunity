//! Same-process Linux sealed image bytes. This grants no activation authority.
//!
//! The host supplies the expected digest and a bounded maximum. Source paths are
//! resolved beneath one canonical directory without symlink traversal. Accepted
//! bytes live in an irreversibly sealed memfd until the lease is dropped.
//! Consumers must retain the lease through loading and all deferred image reads.
//! CLOEXEC deliberately excludes cross-exec descriptor transfer. Transitive
//! dependencies and project/provider filesystem trees are outside this contract.

#[cfg(target_os = "linux")]
#[path = "image_lease/capture.rs"]
mod capture;
#[cfg(target_os = "linux")]
#[path = "image_lease/identity.rs"]
mod identity;

#[cfg(target_os = "linux")]
use std::{
    fs::File,
    marker::PhantomData,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
};

/// Hard per-image limit, independent of the caller's declared maximum.
pub const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// Owned immutable bytes, with no serialization, cloning or readiness token.
///
/// ```compile_fail
/// use compat_player::image_lease::SealedImage;
/// fn clone_authority(image: &SealedImage) -> SealedImage { image.clone() }
/// ```
///
/// ```compile_fail
/// use compat_player::image_lease::SealedImage;
/// let image: SealedImage = serde_json::from_str("{}").unwrap();
/// ```
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct SealedImage {
    file: File,
    digest: String,
    length: u64,
    maximum: u64,
}

/// A same-process loader path borrowed from its descriptor owner.
/// Copying the path does not extend the descriptor's lifetime.
///
/// ```compile_fail
/// use compat_player::image_lease::SealedImage;
/// use std::path::Path;
/// let borrowed = {
///     let owner = SealedImage::capture(Path::new("."), Path::new("image"), &"0".repeat(64), 8).unwrap();
///     owner.load_path()
/// };
/// println!("{}", borrowed.as_path().display());
/// ```
#[cfg(target_os = "linux")]
#[derive(Debug)]
pub struct ImageLoadPath<'a> {
    path: PathBuf,
    _owner: PhantomData<&'a SealedImage>,
}

#[cfg(target_os = "linux")]
impl SealedImage {
    /// Capture exactly one regular, nonempty image beneath `root`.
    ///
    /// `expected_sha256` is a host-selected identity, exactly 64 lowercase hex
    /// digits. It is never derived from fixture authority. `maximum` must be in
    /// `1..=MAX_IMAGE_BYTES`. Unsupported `openat2`/sealing kernels fail closed.
    pub fn capture(
        root: &Path,
        relative: &Path,
        expected_sha256: &str,
        maximum: u64,
    ) -> Result<Self, String> {
        capture::capture(root, relative, expected_sha256, maximum)
    }

    pub fn sha256(&self) -> &str {
        &self.digest
    }

    pub fn len(&self) -> u64 {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn maximum(&self) -> u64 {
        self.maximum
    }

    pub fn load_path(&self) -> ImageLoadPath<'_> {
        ImageLoadPath {
            path: PathBuf::from(format!("/proc/self/fd/{}", self.file.as_raw_fd())),
            _owner: PhantomData,
        }
    }
}

#[cfg(target_os = "linux")]
impl ImageLoadPath<'_> {
    pub fn as_path(&self) -> &Path {
        &self.path
    }
}
