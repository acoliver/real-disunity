use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use thiserror::Error;
use worldrunner_asset_id::{
    ArtifactKind, AssetVariant, ContentHash, LogicalAssetId, SourceProvenance,
};

const MAGIC: [u8; 8] = *b"RDPACK01";
const FORMAT_VERSION: u32 = 2;
const HEADER_LENGTH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PackageKind {
    Base,
    Patch,
    Dlc,
    Mod,
    Development,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub package_id: String,
    pub semantic_version: String,
    pub package_kind: PackageKind,
    pub content_schema: u32,
}

#[derive(Clone, Debug)]
pub struct AssetInput {
    pub logical_id: LogicalAssetId,
    pub variant: AssetVariant,
    pub kind: ArtifactKind,
    pub dependencies: Vec<LogicalAssetId>,
    pub provenance: SourceProvenance,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AssetKey {
    pub logical_id: LogicalAssetId,
    pub variant: AssetVariant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChunkCodec {
    Raw,
    Zstd,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageChunk {
    pub offset: u64,
    pub stored_length: u64,
    pub uncompressed_length: u64,
    pub codec: ChunkCodec,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageEntry {
    pub key: AssetKey,
    pub kind: ArtifactKind,
    pub content_hash: ContentHash,
    pub uncompressed_length: u64,
    pub chunks: Vec<PackageChunk>,
    pub dependencies: Vec<LogicalAssetId>,
    pub provenance: SourceProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct PackageIndex {
    manifest: PackageManifest,
    entries: Vec<PackageEntry>,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildPolicy {
    pub chunk_bytes: usize,
    pub zstd_level: i32,
    pub minimum_compression_savings: usize,
}

impl Default for BuildPolicy {
    fn default() -> Self {
        Self {
            chunk_bytes: 256 * 1024,
            zstd_level: 9,
            minimum_compression_savings: 32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PackageLimits {
    pub max_package_bytes: usize,
    pub max_index_bytes: usize,
    pub max_asset_bytes: usize,
    pub max_entries: usize,
    pub max_chunks_per_asset: usize,
    pub max_total_chunks: usize,
}

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_package_bytes: 8 * 1024 * 1024 * 1024,
            max_index_bytes: 64 * 1024 * 1024,
            max_asset_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 1_000_000,
            max_chunks_per_asset: 65_536,
            max_total_chunks: 4_000_000,
        }
    }
}

pub struct PackageBuilder {
    manifest: PackageManifest,
    policy: BuildPolicy,
    assets: Vec<AssetInput>,
}

impl PackageBuilder {
    pub fn new(manifest: PackageManifest) -> Self {
        Self {
            manifest,
            policy: BuildPolicy::default(),
            assets: Vec::new(),
        }
    }

    pub const fn with_policy(manifest: PackageManifest, policy: BuildPolicy) -> Self {
        Self {
            manifest,
            policy,
            assets: Vec::new(),
        }
    }

    pub fn add(&mut self, asset: AssetInput) {
        self.assets.push(asset);
    }

    pub fn build(mut self) -> Result<BuiltPackage, PackageError> {
        if self.policy.chunk_bytes == 0 {
            return Err(PackageError::InvalidBuildPolicy);
        }
        order_and_validate(&mut self.assets)?;
        let encoded_assets = encode_unique_payloads(&self.assets, self.policy)?;

        let mut output = vec![0_u8; HEADER_LENGTH];
        let stored_chunks = append_chunk_payloads(&mut output, encoded_assets)?;
        let entries = describe_entries(self.assets, &stored_chunks)?;

        let index = postcard::to_allocvec(&PackageIndex {
            manifest: self.manifest,
            entries,
        })?;
        let index_offset = u64::try_from(output.len()).map_err(|_| PackageError::SizeOverflow)?;
        let index_length = u64::try_from(index.len()).map_err(|_| PackageError::SizeOverflow)?;
        let index_hash = ContentHash::of(&index);
        output.extend_from_slice(&index);
        write_header(
            &mut output[..HEADER_LENGTH],
            index_offset,
            index_length,
            index_hash,
        );
        let package_hash = ContentHash::of(&output);
        Ok(BuiltPackage {
            bytes: output,
            package_hash,
        })
    }
}

/// Sorts assets into their canonical order and rejects the two shapes a package
/// cannot represent: a repeated key, and an asset depending on itself.
fn order_and_validate(assets: &mut [AssetInput]) -> Result<(), PackageError> {
    assets.sort_by(|left, right| {
        (&left.logical_id, &left.variant).cmp(&(&right.logical_id, &right.variant))
    });
    let mut seen_keys = BTreeSet::new();
    for asset in assets.iter_mut() {
        let key = AssetKey {
            logical_id: asset.logical_id.clone(),
            variant: asset.variant.clone(),
        };
        if !seen_keys.insert(key.clone()) {
            return Err(PackageError::DuplicateAsset(key));
        }
        asset.dependencies.sort();
        asset.dependencies.dedup();
        if asset.dependencies.contains(&asset.logical_id) {
            return Err(PackageError::SelfDependency(asset.logical_id.clone()));
        }
    }
    Ok(())
}

/// Encodes each distinct payload once, so assets sharing bytes share chunks.
fn encode_unique_payloads(
    assets: &[AssetInput],
    policy: BuildPolicy,
) -> Result<BTreeMap<ContentHash, EncodedAsset>, PackageError> {
    let mut encoded_assets = BTreeMap::<ContentHash, EncodedAsset>::new();
    for asset in assets {
        let hash = ContentHash::of(&asset.bytes);
        if let std::collections::btree_map::Entry::Vacant(entry) = encoded_assets.entry(hash) {
            entry.insert(encode_asset(&asset.bytes, policy)?);
        }
    }
    Ok(encoded_assets)
}

/// Writes every encoded chunk into the output buffer, returning the descriptors
/// that record where each one landed.
fn append_chunk_payloads(
    output: &mut Vec<u8>,
    encoded_assets: BTreeMap<ContentHash, EncodedAsset>,
) -> Result<BTreeMap<ContentHash, Vec<PackageChunk>>, PackageError> {
    let mut stored_chunks = BTreeMap::<ContentHash, Vec<PackageChunk>>::new();
    for (hash, encoded) in encoded_assets {
        let mut descriptors = Vec::with_capacity(encoded.chunks.len());
        for chunk in encoded.chunks {
            let offset = u64::try_from(output.len()).map_err(|_| PackageError::SizeOverflow)?;
            let stored_length =
                u64::try_from(chunk.bytes.len()).map_err(|_| PackageError::SizeOverflow)?;
            output.extend_from_slice(&chunk.bytes);
            descriptors.push(PackageChunk {
                offset,
                stored_length,
                uncompressed_length: chunk.uncompressed_length,
                codec: chunk.codec,
            });
        }
        stored_chunks.insert(hash, descriptors);
    }
    Ok(stored_chunks)
}

fn describe_entries(
    assets: Vec<AssetInput>,
    stored_chunks: &BTreeMap<ContentHash, Vec<PackageChunk>>,
) -> Result<Vec<PackageEntry>, PackageError> {
    assets
        .into_iter()
        .map(|asset| {
            let content_hash = ContentHash::of(&asset.bytes);
            let length =
                u64::try_from(asset.bytes.len()).map_err(|_| PackageError::SizeOverflow)?;
            Ok(PackageEntry {
                key: AssetKey {
                    logical_id: asset.logical_id,
                    variant: asset.variant,
                },
                kind: asset.kind,
                content_hash,
                uncompressed_length: length,
                chunks: stored_chunks[&content_hash].clone(),
                dependencies: asset.dependencies,
                provenance: asset.provenance,
            })
        })
        .collect()
}

struct EncodedAsset {
    chunks: Vec<EncodedChunk>,
}

struct EncodedChunk {
    bytes: Vec<u8>,
    uncompressed_length: u64,
    codec: ChunkCodec,
}

fn encode_asset(bytes: &[u8], policy: BuildPolicy) -> Result<EncodedAsset, PackageError> {
    let mut chunks = Vec::new();
    for source in bytes.chunks(policy.chunk_bytes) {
        let compressed = zstd::bulk::compress(source, policy.zstd_level)?;
        let use_compressed = compressed
            .len()
            .checked_add(policy.minimum_compression_savings)
            .is_some_and(|threshold| threshold < source.len());
        chunks.push(EncodedChunk {
            bytes: if use_compressed {
                compressed
            } else {
                source.to_vec()
            },
            uncompressed_length: u64::try_from(source.len())
                .map_err(|_| PackageError::SizeOverflow)?,
            codec: if use_compressed {
                ChunkCodec::Zstd
            } else {
                ChunkCodec::Raw
            },
        });
    }
    Ok(EncodedAsset { chunks })
}

pub struct BuiltPackage {
    pub bytes: Vec<u8>,
    pub package_hash: ContentHash,
}

pub struct Package {
    bytes: Vec<u8>,
    manifest: PackageManifest,
    entries: Vec<PackageEntry>,
}

impl Package {
    pub fn parse(bytes: Vec<u8>, limits: PackageLimits) -> Result<Self, PackageError> {
        if bytes.len() > limits.max_package_bytes {
            return Err(PackageError::LimitExceeded("package bytes"));
        }
        if bytes.len() < HEADER_LENGTH {
            return Err(PackageError::TruncatedHeader);
        }
        if bytes[..8] != MAGIC {
            return Err(PackageError::InvalidMagic);
        }
        let version = read_u32(&bytes, 8)?;
        if version != FORMAT_VERSION {
            return Err(PackageError::UnsupportedVersion(version));
        }
        let index_offset =
            usize::try_from(read_u64(&bytes, 16)?).map_err(|_| PackageError::SizeOverflow)?;
        let index_length =
            usize::try_from(read_u64(&bytes, 24)?).map_err(|_| PackageError::SizeOverflow)?;
        if index_length > limits.max_index_bytes {
            return Err(PackageError::LimitExceeded("index bytes"));
        }
        let index_end = index_offset
            .checked_add(index_length)
            .ok_or(PackageError::SizeOverflow)?;
        if index_offset < HEADER_LENGTH || index_end != bytes.len() {
            return Err(PackageError::InvalidIndexExtent);
        }
        let expected_index_hash = ContentHash(
            bytes[32..64]
                .try_into()
                .map_err(|_| PackageError::TruncatedHeader)?,
        );
        let index_bytes = &bytes[index_offset..index_end];
        if ContentHash::of(index_bytes) != expected_index_hash {
            return Err(PackageError::IndexHashMismatch);
        }
        let index: PackageIndex = postcard::from_bytes(index_bytes)?;
        if index.entries.len() > limits.max_entries {
            return Err(PackageError::LimitExceeded("entry count"));
        }
        validate_entries(&index.entries, index_offset, bytes.len(), limits)?;
        Ok(Self {
            bytes,
            manifest: index.manifest,
            entries: index.entries,
        })
    }

    pub const fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    pub fn read(&self, key: &AssetKey) -> Result<Vec<u8>, PackageError> {
        let index = self
            .entries
            .binary_search_by(|entry| entry.key.cmp(key))
            .map_err(|_| PackageError::AssetNotFound)?;
        let entry = &self.entries[index];
        let capacity =
            usize::try_from(entry.uncompressed_length).map_err(|_| PackageError::SizeOverflow)?;
        let mut output = Vec::with_capacity(capacity);
        for chunk in &entry.chunks {
            let start = usize::try_from(chunk.offset).map_err(|_| PackageError::SizeOverflow)?;
            let stored_length =
                usize::try_from(chunk.stored_length).map_err(|_| PackageError::SizeOverflow)?;
            let end = start
                .checked_add(stored_length)
                .ok_or(PackageError::SizeOverflow)?;
            let stored = self
                .bytes
                .get(start..end)
                .ok_or(PackageError::InvalidBlobExtent)?;
            let uncompressed_length = usize::try_from(chunk.uncompressed_length)
                .map_err(|_| PackageError::SizeOverflow)?;
            match chunk.codec {
                ChunkCodec::Raw => output.extend_from_slice(stored),
                ChunkCodec::Zstd => {
                    let decoded = zstd::bulk::decompress(stored, uncompressed_length)?;
                    if decoded.len() != uncompressed_length {
                        return Err(PackageError::ChunkLengthMismatch);
                    }
                    output.extend_from_slice(&decoded);
                }
            }
        }
        if output.len() != capacity {
            return Err(PackageError::AssetLengthMismatch(entry.key.clone()));
        }
        if ContentHash::of(&output) != entry.content_hash {
            return Err(PackageError::BlobHashMismatch(entry.key.clone()));
        }
        Ok(output)
    }
}

pub struct FilePackage {
    file: File,
    file_length: u64,
    manifest: PackageManifest,
    entries: Vec<PackageEntry>,
}

impl FilePackage {
    pub fn open(path: impl AsRef<Path>, limits: PackageLimits) -> Result<Self, PackageError> {
        let mut file = File::open(path)?;
        let file_length = file.metadata()?.len();
        let package_length =
            usize::try_from(file_length).map_err(|_| PackageError::SizeOverflow)?;
        if package_length > limits.max_package_bytes {
            return Err(PackageError::LimitExceeded("package bytes"));
        }
        if package_length < HEADER_LENGTH {
            return Err(PackageError::TruncatedHeader);
        }

        let mut header = [0_u8; HEADER_LENGTH];
        file.read_exact(&mut header)?;
        if header[..8] != MAGIC {
            return Err(PackageError::InvalidMagic);
        }
        let version = read_u32(&header, 8)?;
        if version != FORMAT_VERSION {
            return Err(PackageError::UnsupportedVersion(version));
        }
        let index_offset =
            usize::try_from(read_u64(&header, 16)?).map_err(|_| PackageError::SizeOverflow)?;
        let index_length =
            usize::try_from(read_u64(&header, 24)?).map_err(|_| PackageError::SizeOverflow)?;
        if index_length > limits.max_index_bytes {
            return Err(PackageError::LimitExceeded("index bytes"));
        }
        let index_end = index_offset
            .checked_add(index_length)
            .ok_or(PackageError::SizeOverflow)?;
        if index_offset < HEADER_LENGTH || index_end != package_length {
            return Err(PackageError::InvalidIndexExtent);
        }
        let expected_index_hash = ContentHash(
            header[32..64]
                .try_into()
                .map_err(|_| PackageError::TruncatedHeader)?,
        );
        file.seek(SeekFrom::Start(
            u64::try_from(index_offset).map_err(|_| PackageError::SizeOverflow)?,
        ))?;
        let mut index_bytes = vec![0_u8; index_length];
        file.read_exact(&mut index_bytes)?;
        if ContentHash::of(&index_bytes) != expected_index_hash {
            return Err(PackageError::IndexHashMismatch);
        }
        let index: PackageIndex = postcard::from_bytes(&index_bytes)?;
        if index.entries.len() > limits.max_entries {
            return Err(PackageError::LimitExceeded("entry count"));
        }
        validate_entries(&index.entries, index_offset, package_length, limits)?;
        Ok(Self {
            file,
            file_length,
            manifest: index.manifest,
            entries: index.entries,
        })
    }

    pub const fn manifest(&self) -> &PackageManifest {
        &self.manifest
    }

    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    pub fn read(&mut self, key: &AssetKey) -> Result<Vec<u8>, PackageError> {
        if self.file.metadata()?.len() != self.file_length {
            return Err(PackageError::PackageChanged);
        }
        let index = self
            .entries
            .binary_search_by(|entry| entry.key.cmp(key))
            .map_err(|_| PackageError::AssetNotFound)?;
        let entry = &self.entries[index];
        let capacity =
            usize::try_from(entry.uncompressed_length).map_err(|_| PackageError::SizeOverflow)?;
        let mut output = Vec::with_capacity(capacity);
        for chunk in &entry.chunks {
            let stored_length =
                usize::try_from(chunk.stored_length).map_err(|_| PackageError::SizeOverflow)?;
            let mut stored = vec![0_u8; stored_length];
            self.file.seek(SeekFrom::Start(chunk.offset))?;
            self.file.read_exact(&mut stored)?;
            let uncompressed_length = usize::try_from(chunk.uncompressed_length)
                .map_err(|_| PackageError::SizeOverflow)?;
            match chunk.codec {
                ChunkCodec::Raw => output.extend_from_slice(&stored),
                ChunkCodec::Zstd => {
                    let decoded = zstd::bulk::decompress(&stored, uncompressed_length)?;
                    if decoded.len() != uncompressed_length {
                        return Err(PackageError::ChunkLengthMismatch);
                    }
                    output.extend_from_slice(&decoded);
                }
            }
        }
        if output.len() != capacity {
            return Err(PackageError::AssetLengthMismatch(entry.key.clone()));
        }
        if ContentHash::of(&output) != entry.content_hash {
            return Err(PackageError::BlobHashMismatch(entry.key.clone()));
        }
        Ok(output)
    }
}

fn validate_entries(
    entries: &[PackageEntry],
    index_offset: usize,
    package_length: usize,
    limits: PackageLimits,
) -> Result<(), PackageError> {
    let mut previous = None;
    let mut total_chunks = 0_usize;
    for entry in entries {
        if let Some(previous) = previous
            && previous >= &entry.key
        {
            return Err(PackageError::UnsortedOrDuplicateEntries);
        }
        previous = Some(&entry.key);
        if entry.dependencies.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(PackageError::UnsortedOrDuplicateDependencies);
        }
        if entry.dependencies.contains(&entry.key.logical_id) {
            return Err(PackageError::SelfDependency(entry.key.logical_id.clone()));
        }
        if entry.chunks.len() > limits.max_chunks_per_asset {
            return Err(PackageError::LimitExceeded("chunks per asset"));
        }
        total_chunks = total_chunks
            .checked_add(entry.chunks.len())
            .ok_or(PackageError::SizeOverflow)?;
        if total_chunks > limits.max_total_chunks {
            return Err(PackageError::LimitExceeded("total chunks"));
        }
        let uncompressed_length =
            usize::try_from(entry.uncompressed_length).map_err(|_| PackageError::SizeOverflow)?;
        if uncompressed_length > limits.max_asset_bytes {
            return Err(PackageError::LimitExceeded("asset bytes"));
        }
        let mut decoded_total = 0_u64;
        for chunk in &entry.chunks {
            if chunk.codec == ChunkCodec::Raw && chunk.stored_length != chunk.uncompressed_length {
                return Err(PackageError::ChunkLengthMismatch);
            }
            decoded_total = decoded_total
                .checked_add(chunk.uncompressed_length)
                .ok_or(PackageError::SizeOverflow)?;
            let start = usize::try_from(chunk.offset).map_err(|_| PackageError::SizeOverflow)?;
            let length =
                usize::try_from(chunk.stored_length).map_err(|_| PackageError::SizeOverflow)?;
            let end = start
                .checked_add(length)
                .ok_or(PackageError::SizeOverflow)?;
            if start < HEADER_LENGTH || end > index_offset || end > package_length {
                return Err(PackageError::InvalidBlobExtent);
            }
        }
        if decoded_total != entry.uncompressed_length {
            return Err(PackageError::AssetLengthMismatch(entry.key.clone()));
        }
    }
    Ok(())
}

fn write_header(output: &mut [u8], index_offset: u64, index_length: u64, hash: ContentHash) {
    output[..8].copy_from_slice(&MAGIC);
    output[8..12].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    output[12..16].copy_from_slice(&0_u32.to_le_bytes());
    output[16..24].copy_from_slice(&index_offset.to_le_bytes());
    output[24..32].copy_from_slice(&index_length.to_le_bytes());
    output[32..64].copy_from_slice(&hash.0);
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    bytes
        .get(offset..offset + 4)
        .ok_or(PackageError::TruncatedHeader)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| PackageError::TruncatedHeader)
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PackageError> {
    bytes
        .get(offset..offset + 8)
        .ok_or(PackageError::TruncatedHeader)?
        .try_into()
        .map(u64::from_le_bytes)
        .map_err(|_| PackageError::TruncatedHeader)
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package serialization failed: {0}")]
    Serialization(#[from] postcard::Error),
    #[error("package I/O or compression failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("package build policy is invalid")]
    InvalidBuildPolicy,
    #[error("package header is truncated")]
    TruncatedHeader,
    #[error("package magic is invalid")]
    InvalidMagic,
    #[error("package format version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("package size arithmetic overflowed")]
    SizeOverflow,
    #[error("package limit exceeded: {0}")]
    LimitExceeded(&'static str),
    #[error("package index extent is invalid")]
    InvalidIndexExtent,
    #[error("package index hash does not match")]
    IndexHashMismatch,
    #[error("package changed after it was opened")]
    PackageChanged,
    #[error("package entries are unsorted or duplicated")]
    UnsortedOrDuplicateEntries,
    #[error("asset dependencies are unsorted or duplicated")]
    UnsortedOrDuplicateDependencies,
    #[error("package contains duplicate asset {0:?}")]
    DuplicateAsset(AssetKey),
    #[error("asset {0} depends on itself")]
    SelfDependency(LogicalAssetId),
    #[error("package blob extent is invalid")]
    InvalidBlobExtent,
    #[error("asset does not exist in package")]
    AssetNotFound,
    #[error("chunk length is inconsistent")]
    ChunkLengthMismatch,
    #[error("asset length does not match for {0:?}")]
    AssetLengthMismatch(AssetKey),
    #[error("asset blob hash does not match for {0:?}")]
    BlobHashMismatch(AssetKey),
}

#[cfg(test)]
mod tests {
    use super::*;
    use worldrunner_asset_id::{ProvenanceClassification, QualityTier, SourceProvenance};

    fn input(id: &str, bytes: &[u8]) -> AssetInput {
        AssetInput {
            logical_id: LogicalAssetId::new(id).unwrap(),
            variant: AssetVariant {
                art_set: "test".to_owned(),
                quality: QualityTier::Baseline,
                platform_profile: "portable".to_owned(),
            },
            kind: ArtifactKind::Data,
            dependencies: Vec::new(),
            provenance: SourceProvenance {
                classification: ProvenanceClassification::Original,
                source_package: "test".to_owned(),
                source_virtual_path: id.to_owned(),
                source_sha256: "00".repeat(32),
                importer_id: "test".to_owned(),
                importer_version: "1".to_owned(),
            },
            bytes: bytes.to_vec(),
        }
    }

    fn manifest() -> PackageManifest {
        PackageManifest {
            package_id: "test.package".to_owned(),
            semantic_version: "0.1.0".to_owned(),
            package_kind: PackageKind::Development,
            content_schema: 1,
        }
    }

    fn key() -> AssetKey {
        AssetKey {
            logical_id: LogicalAssetId::new("test.alpha").unwrap(),
            variant: AssetVariant {
                art_set: "test".to_owned(),
                quality: QualityTier::Baseline,
                platform_profile: "portable".to_owned(),
            },
        }
    }

    #[test]
    fn build_is_independent_of_input_order() {
        let mut left = PackageBuilder::new(manifest());
        left.add(input("test.alpha", b"alpha"));
        left.add(input("test.beta", b"beta"));
        let mut right = PackageBuilder::new(manifest());
        right.add(input("test.beta", b"beta"));
        right.add(input("test.alpha", b"alpha"));
        assert_eq!(left.build().unwrap().bytes, right.build().unwrap().bytes);
    }

    #[test]
    fn package_validates_and_reads_assets() {
        let mut builder = PackageBuilder::new(manifest());
        builder.add(input("test.alpha", b"alpha"));
        let built = builder.build().unwrap();
        let package = Package::parse(built.bytes, PackageLimits::default()).unwrap();
        assert_eq!(package.read(&key()).unwrap(), b"alpha");
    }

    #[test]
    fn compresses_and_reassembles_independent_chunks() {
        let bytes = vec![0x5a; 600_000];
        let mut builder = PackageBuilder::with_policy(
            manifest(),
            BuildPolicy {
                chunk_bytes: 128 * 1024,
                ..BuildPolicy::default()
            },
        );
        builder.add(input("test.alpha", &bytes));
        let built = builder.build().unwrap();
        assert!(built.bytes.len() < bytes.len() / 10);
        let package = Package::parse(built.bytes, PackageLimits::default()).unwrap();
        let entry = &package.entries()[0];
        assert!(entry.chunks.len() > 1);
        assert!(
            entry
                .chunks
                .iter()
                .all(|chunk| chunk.codec == ChunkCodec::Zstd)
        );
        assert_eq!(package.read(&key()).unwrap(), bytes);
    }

    #[test]
    fn dependencies_are_sorted_and_deduplicated() {
        let mut asset = input("test.alpha", b"alpha");
        asset.dependencies = vec![
            LogicalAssetId::new("test.gamma").unwrap(),
            LogicalAssetId::new("test.beta").unwrap(),
            LogicalAssetId::new("test.gamma").unwrap(),
        ];
        let mut builder = PackageBuilder::new(manifest());
        builder.add(asset);
        let package =
            Package::parse(builder.build().unwrap().bytes, PackageLimits::default()).unwrap();
        assert_eq!(
            package.entries()[0].dependencies,
            vec![
                LogicalAssetId::new("test.beta").unwrap(),
                LogicalAssetId::new("test.gamma").unwrap(),
            ]
        );
    }

    #[test]
    fn file_backed_package_reads_only_requested_assets() {
        let mut builder = PackageBuilder::new(manifest());
        builder.add(input("test.alpha", b"alpha"));
        builder.add(input("test.beta", &vec![0x42; 300_000]));
        let bytes = builder.build().unwrap().bytes;
        let path = std::env::temp_dir().join(format!(
            "real-disunity-package-file-read-{}.rdpack",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        let mut package = FilePackage::open(&path, PackageLimits::default()).unwrap();
        assert_eq!(package.entries().len(), 2);
        assert_eq!(package.read(&key()).unwrap(), b"alpha");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn tampered_index_is_rejected() {
        let mut builder = PackageBuilder::new(manifest());
        builder.add(input("test.alpha", b"alpha"));
        let mut bytes = builder.build().unwrap().bytes;
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(matches!(
            Package::parse(bytes, PackageLimits::default()),
            Err(PackageError::IndexHashMismatch)
        ));
    }
}
