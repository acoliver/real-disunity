use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::{ExpectedArtifact, PublicationLimits, PublicationVerificationError};
use crate::STREAM_CHUNK_BYTES;

pub(super) fn read_bounded(
    root: &Path,
    logical_path: &str,
    maximum: u64,
) -> Result<Vec<u8>, PublicationVerificationError> {
    let root = canonical_directory(root)?;
    require_regular_member_path(&root, logical_path)?;
    let path = member_path(&root, logical_path);
    let metadata =
        fs::symlink_metadata(&path).map_err(|source| io("inspect member", &path, source))?;
    if !metadata.file_type().is_file() {
        return Err(PublicationVerificationError::UnexpectedNode {
            path: logical_path.to_owned(),
        });
    }
    let mut file = File::open(&path).map_err(|source| io("open member", &path, source))?;
    let mut bytes = Vec::new();
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    let mut total = 0_u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| io("read member", &path, source))?;
        if count == 0 {
            break;
        }
        total = checked_add(total, count)?;
        if total > maximum {
            return Err(PublicationVerificationError::LimitExceeded {
                limit: "artifact byte length",
                observed: total,
                maximum,
            });
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(bytes)
}

/// Opens one verified member for streamed reading, rechecking member shape.
///
/// Ancestor containment and regular-file shape are rechecked from the canonical
/// root so a post-verification swap to a link or directory fails closed. Byte
/// verification stays with the caller's single pass over the returned stream.
pub(super) fn open_member(
    root: &Path,
    logical_path: &str,
) -> Result<File, PublicationVerificationError> {
    require_regular_member_path(root, logical_path)?;
    let path = member_path(root, logical_path);
    let metadata =
        fs::symlink_metadata(&path).map_err(|source| io("inspect member", &path, source))?;
    if !metadata.file_type().is_file() {
        return Err(PublicationVerificationError::UnexpectedNode {
            path: logical_path.to_owned(),
        });
    }
    File::open(&path).map_err(|source| io("open member", &path, source))
}

pub(super) fn verify_exact(
    root: &Path,
    expected: &BTreeMap<String, ExpectedArtifact>,
    limits: PublicationLimits,
) -> Result<PathBuf, PublicationVerificationError> {
    let root = canonical_directory(root)?;
    let expected_dirs = expected_directories(expected.keys());
    let mut observed_files = BTreeSet::new();
    let mut observed_dirs = BTreeSet::new();
    collect_nodes(&root, &mut observed_files, &mut observed_dirs, limits)?;
    require_exact_nodes(expected, &expected_dirs, &observed_files, &observed_dirs)?;
    let mut total = 0_u64;
    for (logical_path, artifact) in expected {
        let path = member_path(&root, logical_path);
        let (bytes, digest) = hash_file(&path, limits.max_artifact_bytes)?;
        require_artifact_match(logical_path, artifact, bytes, &digest)?;
        total = total
            .checked_add(bytes)
            .ok_or(PublicationVerificationError::ByteCountOverflow)?;
    }
    if total > limits.max_total_bytes {
        return Err(PublicationVerificationError::LimitExceeded {
            limit: "total byte length",
            observed: total,
            maximum: limits.max_total_bytes,
        });
    }
    Ok(root)
}

fn collect_nodes(
    root: &Path,
    files: &mut BTreeSet<String>,
    directories: &mut BTreeSet<String>,
    limits: PublicationLimits,
) -> Result<(), PublicationVerificationError> {
    let mut pending = vec![(root.to_owned(), 0_usize)];
    let mut node_count = 0_usize;
    while let Some((directory, depth)) = pending.pop() {
        let entries =
            fs::read_dir(&directory).map_err(|source| io("read directory", &directory, source))?;
        for entry in entries {
            let entry = entry.map_err(|source| io("read directory entry", &directory, source))?;
            node_count = node_count
                .checked_add(1)
                .ok_or(PublicationVerificationError::ByteCountOverflow)?;
            require_traversal_limit("filesystem node count", node_count, limits.max_nodes)?;
            let entry_depth = depth
                .checked_add(1)
                .ok_or(PublicationVerificationError::ByteCountOverflow)?;
            require_traversal_limit("filesystem depth", entry_depth, limits.max_depth)?;
            collect_entry(root, &entry, entry_depth, files, directories, &mut pending)?;
        }
    }
    Ok(())
}

fn collect_entry(
    root: &Path,
    entry: &fs::DirEntry,
    depth: usize,
    files: &mut BTreeSet<String>,
    directories: &mut BTreeSet<String>,
    pending: &mut Vec<(PathBuf, usize)>,
) -> Result<(), PublicationVerificationError> {
    let path = entry.path();
    let relative = relative_logical_path(root, &path)?;
    let file_type = entry
        .file_type()
        .map_err(|source| io("inspect node", &path, source))?;
    if file_type.is_file() {
        files.insert(relative);
    } else if file_type.is_dir() {
        directories.insert(relative);
        pending.push((path, depth));
    } else {
        return Err(PublicationVerificationError::UnexpectedNode { path: relative });
    }
    Ok(())
}

fn require_traversal_limit(
    limit: &'static str,
    observed: usize,
    maximum: usize,
) -> Result<(), PublicationVerificationError> {
    if observed > maximum {
        return Err(PublicationVerificationError::LimitExceeded {
            limit,
            observed: u64::try_from(observed)
                .map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
            maximum: u64::try_from(maximum)
                .map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
        });
    }
    Ok(())
}

fn require_exact_nodes(
    expected: &BTreeMap<String, ExpectedArtifact>,
    expected_dirs: &BTreeSet<String>,
    files: &BTreeSet<String>,
    directories: &BTreeSet<String>,
) -> Result<(), PublicationVerificationError> {
    let expected_files: BTreeSet<String> = expected.keys().cloned().collect();
    if let Some(path) = files.difference(&expected_files).next() {
        return Err(PublicationVerificationError::UnexpectedNode { path: path.clone() });
    }
    if let Some(path) = directories.difference(expected_dirs).next() {
        return Err(PublicationVerificationError::UnexpectedNode { path: path.clone() });
    }
    if let Some(path) = expected_files.difference(files).next() {
        return Err(PublicationVerificationError::MissingNode { path: path.clone() });
    }
    if let Some(path) = expected_dirs.difference(directories).next() {
        return Err(PublicationVerificationError::MissingNode { path: path.clone() });
    }
    Ok(())
}

fn require_artifact_match(
    logical_path: &str,
    expected: &ExpectedArtifact,
    bytes: u64,
    digest: &str,
) -> Result<(), PublicationVerificationError> {
    if bytes != expected.bytes {
        return Err(PublicationVerificationError::ByteLengthMismatch {
            path: logical_path.to_owned(),
            expected: expected.bytes,
            observed: bytes,
        });
    }
    if digest != expected.sha256 {
        return Err(PublicationVerificationError::DigestMismatch {
            path: logical_path.to_owned(),
            expected: expected.sha256.clone(),
            observed: digest.to_owned(),
        });
    }
    Ok(())
}

fn hash_file(path: &Path, maximum: u64) -> Result<(u64, String), PublicationVerificationError> {
    let mut file = File::open(path).map_err(|source| io("open artifact", path, source))?;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    let mut bytes = 0_u64;
    let mut hasher = Sha256::new();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|source| io("read artifact", path, source))?;
        if count == 0 {
            break;
        }
        bytes = checked_add(bytes, count)?;
        if bytes > maximum {
            return Err(PublicationVerificationError::LimitExceeded {
                limit: "artifact byte length",
                observed: bytes,
                maximum,
            });
        }
        hasher.update(&buffer[..count]);
    }
    Ok((bytes, digest_hex(hasher.finalize())))
}

fn require_regular_member_path(
    root: &Path,
    logical_path: &str,
) -> Result<(), PublicationVerificationError> {
    let segments: Vec<&str> = logical_path.split('/').collect();
    let mut current = root.to_owned();
    for segment in segments.iter().take(segments.len().saturating_sub(1)) {
        current.push(segment);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|source| io("inspect member directory", &current, source))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(PublicationVerificationError::UnexpectedNode {
                path: logical_path.to_owned(),
            });
        }
    }
    Ok(())
}

fn canonical_directory(path: &Path) -> Result<PathBuf, PublicationVerificationError> {
    let canonical =
        fs::canonicalize(path).map_err(|source| io("canonicalize root", path, source))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|source| io("inspect canonical root", &canonical, source))?;
    if !metadata.is_dir() {
        return Err(PublicationVerificationError::NotDirectory { path: canonical });
    }
    Ok(canonical)
}

fn expected_directories<'a>(paths: impl Iterator<Item = &'a String>) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path.split('/').collect();
        segments.pop();
        while !segments.is_empty() {
            directories.insert(segments.join("/"));
            segments.pop();
        }
    }
    directories
}

fn relative_logical_path(root: &Path, path: &Path) -> Result<String, PublicationVerificationError> {
    let relative =
        path.strip_prefix(root)
            .map_err(|_| PublicationVerificationError::UnexpectedNode {
                path: path.display().to_string(),
            })?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn checked_add(total: u64, count: usize) -> Result<u64, PublicationVerificationError> {
    total
        .checked_add(
            u64::try_from(count).map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
        )
        .ok_or(PublicationVerificationError::ByteCountOverflow)
}

fn member_path(root: &Path, logical_path: &str) -> PathBuf {
    root.join(logical_path.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn digest_hex(digest: impl AsRef<[u8]>) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest.as_ref() {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn io(
    operation: &'static str,
    path: &Path,
    source: std::io::Error,
) -> PublicationVerificationError {
    PublicationVerificationError::Io {
        operation,
        path: path.to_owned(),
        source,
    }
}
