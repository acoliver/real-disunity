//! Patch overlays for a game's original script files.
//!
//! A game keeps its original scripts unchanged and ships ordered patches beside them, for
//! security fixes and Lua 5.4 updates. Each file's patches are pinned to the SHA-256 of the
//! original bytes, as `sha256sum` prints it. When the original changes, loading the file fails and
//! names both hashes, so the patches are reviewed against the new file; they are never applied to
//! it, and the file never runs unpatched in their place.
//!
//! An edit replaces one exact byte sequence with another and must match exactly once in the text
//! as it stands when the edit applies. Edits apply in order within a patch, and patches apply in
//! order. Matching is byte for byte: line endings and encodings are not normalized.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

/// One exact replacement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Edit {
    pub find: String,
    pub replace: String,
}

/// A named, ordered group of edits, such as one security fix.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Patch {
    pub name: String,
    pub edits: Vec<Edit>,
}

/// The patches for one original file, pinned to the file's original bytes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct PatchSet {
    /// The file's name as the game's sources know it.
    pub file: String,
    /// Lowercase hexadecimal SHA-256 of the original file.
    pub original_sha256: String,
    pub patches: Vec<Patch>,
}

/// Every patch set a game ships, by file.
#[derive(Clone, Debug, Default)]
pub struct Overlay {
    sets: BTreeMap<String, PatchSet>,
}

/// An original file after its patches, with the patches applied in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patched {
    pub source: Vec<u8>,
    pub applied: Vec<String>,
}

/// Why a patch set was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    /// The original no longer has the pinned hash; its patches need review.
    Stale {
        file: String,
        pinned: String,
        found: String,
    },
    /// An edit's text occurs `count` times where it must occur exactly once.
    Match {
        file: String,
        patch: String,
        edit: usize,
        count: usize,
    },
    /// The overlay is malformed: a duplicate file or an invalid pin.
    Invalid { file: String, message: String },
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale {
                file,
                pinned,
                found,
            } => write!(
                f,
                "{file}: original changed (patches pinned to sha256 {pinned}, file is {found}); review its patches"
            ),
            Self::Match {
                file,
                patch,
                edit,
                count,
            } => write!(
                f,
                "{file}: patch `{patch}` edit {edit} matches {count} times; it must match exactly once"
            ),
            Self::Invalid { file, message } => write!(f, "{file}: invalid patch set: {message}"),
        }
    }
}

impl std::error::Error for PatchError {}

impl Overlay {
    /// Builds an overlay from patch sets, refusing duplicate files and malformed pins.
    pub fn new(sets: impl IntoIterator<Item = PatchSet>) -> Result<Self, PatchError> {
        let mut overlay = Self::default();
        for set in sets {
            let valid = set.original_sha256.len() == 64
                && set
                    .original_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !valid {
                return Err(invalid(&set.file, "the pin is not 64 lowercase hex digits"));
            }
            if overlay.sets.contains_key(&set.file) {
                return Err(invalid(&set.file, "the file has two patch sets"));
            }
            overlay.sets.insert(set.file.clone(), set);
        }
        Ok(overlay)
    }

    /// The files this overlay patches.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.sets.keys().map(String::as_str)
    }

    /// Applies `file`'s patches to its `original` bytes. A file without patches loads unchanged.
    pub fn apply(&self, file: &str, original: &[u8]) -> Result<Patched, PatchError> {
        let Some(set) = self.sets.get(file) else {
            return Ok(Patched {
                source: original.to_vec(),
                applied: Vec::new(),
            });
        };
        let found = sha256_hex(original);
        if found != set.original_sha256 {
            return Err(PatchError::Stale {
                file: file.into(),
                pinned: set.original_sha256.clone(),
                found,
            });
        }
        let mut source = original.to_vec();
        for patch in &set.patches {
            for (index, edit) in patch.edits.iter().enumerate() {
                source = replace_once(&source, edit).map_err(|count| PatchError::Match {
                    file: file.into(),
                    patch: patch.name.clone(),
                    edit: index,
                    count,
                })?;
            }
        }
        Ok(Patched {
            source,
            applied: set.patches.iter().map(|p| p.name.clone()).collect(),
        })
    }

    /// Every patch set that would fail against the game's current files, for review in one pass.
    /// `read` returns a file's original bytes, or `None` when the game no longer has it.
    pub fn review(&self, read: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<PatchError> {
        self.sets
            .keys()
            .filter_map(|file| match read(file) {
                Some(original) => self.apply(file, &original).err(),
                None => Some(invalid(file, "the original file is missing")),
            })
            .collect()
    }
}

/// Lowercase hexadecimal SHA-256, as `sha256sum` prints it.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `source` with the edit's single match replaced, or the number of matches when it is not one.
fn replace_once(source: &[u8], edit: &Edit) -> Result<Vec<u8>, usize> {
    let find = edit.find.as_bytes();
    if find.is_empty() {
        return Err(0);
    }
    let starts: Vec<usize> = source
        .windows(find.len())
        .enumerate()
        .filter(|(_, window)| *window == find)
        .map(|(start, _)| start)
        .collect();
    // Overlapping occurrences count too, so an ambiguous edit is never applied.
    let [start] = starts[..] else {
        return Err(starts.len());
    };
    let mut patched = Vec::with_capacity(source.len() - find.len() + edit.replace.len());
    patched.extend_from_slice(&source[..start]);
    patched.extend_from_slice(edit.replace.as_bytes());
    patched.extend_from_slice(&source[start + find.len()..]);
    Ok(patched)
}

fn invalid(file: &str, message: &str) -> PatchError {
    PatchError::Invalid {
        file: file.into(),
        message: message.into(),
    }
}
