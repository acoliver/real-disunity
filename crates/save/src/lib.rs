use atomic_write_file::AtomicWriteFile;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use worldrunner_core_types::{SchemaVersion, WorldStateHash};
use worldrunner_world_schema::{CURRENT_WORLD_SCHEMA, WorldDocument, canonical_world_hash};

// Distinct magic prevents old schema-1 envelopes from being mistaken for fresh v1.
const MAGIC: [u8; 8] = *b"WRSAVE01";
const FORMAT_VERSION: u32 = 1;
const FLAGS_NONE: u32 = 0;
const HEADER_PREFIX_LENGTH: usize = 96;
const HEADER_LENGTH: usize = 128;
const DIGEST_LENGTH: usize = 32;
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum SaveError {
    #[error("save I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("save serialization failed: {0}")]
    Encode(#[from] postcard::Error),
    #[error("save is truncated")]
    Truncated,
    #[error("save magic does not match")]
    InvalidMagic,
    #[error("save format version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("save schema version {0} is unsupported")]
    UnsupportedSchema(u32),
    #[error("save flags {0:#010x} are unsupported")]
    UnsupportedFlags(u32),
    #[error("save payload length {actual} exceeds limit {limit}")]
    PayloadTooLarge { actual: usize, limit: usize },
    #[error("save payload length is inconsistent")]
    InvalidLength,
    #[error("save header digest does not match")]
    HeaderDigestMismatch,
    #[error("save payload digest does not match")]
    PayloadDigestMismatch,
    #[error("world document is invalid after decode: {0}")]
    InvalidWorld(String),
    #[error("canonical world hash does not match save metadata")]
    CanonicalHashMismatch,
    #[error("transaction verification failed: {0}")]
    TransactionVerification(String),
    #[error("both primary save and backup are unavailable or invalid")]
    RecoveryFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadSource {
    Primary,
    Backup,
}

#[derive(Debug, PartialEq)]
pub struct RecoveredWorld {
    pub world: WorldDocument,
    pub source: LoadSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SaveHeader {
    schema_version: u32,
    generator_version: u32,
    flags: u32,
    canonical_hash: WorldStateHash,
    payload_digest: [u8; DIGEST_LENGTH],
}

pub fn encode_world(world: &WorldDocument) -> Result<Vec<u8>, SaveError> {
    world
        .validate()
        .map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    let canonical_hash =
        canonical_world_hash(world).map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    let payload = postcard::to_allocvec(world)?;
    encode_payload(
        world.schema_version,
        world.generator_version.0,
        canonical_hash,
        &payload,
    )
}

pub fn decode_world(bytes: &[u8], max_payload_bytes: usize) -> Result<WorldDocument, SaveError> {
    if bytes.len() < MAGIC.len() + size_of::<u32>() {
        return Err(SaveError::Truncated);
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(SaveError::InvalidMagic);
    }
    let version = read_u32(bytes, 8);
    if version != FORMAT_VERSION {
        return Err(SaveError::UnsupportedVersion(version));
    }
    decode_current_world(bytes, max_payload_bytes)
}

pub fn save_world_transactional(path: &Path, world: &WorldDocument) -> Result<(), SaveError> {
    let encoded = encode_world(world)?;
    if let Some(parent) = non_empty_parent(path) {
        fs::create_dir_all(parent)?;
    }
    let expected_hash =
        canonical_world_hash(world).map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    let mut options = AtomicWriteFile::options();
    options.read(true);
    let mut transaction = options.open(path)?;
    transaction.write_all(&encoded)?;
    transaction.sync_all()?;
    let staged_bytes = transaction_bytes(&mut transaction)?;
    let verification = decode_world(&staged_bytes, DEFAULT_MAX_PAYLOAD_BYTES)?;
    let verified_hash = canonical_world_hash(&verification)
        .map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    if verified_hash != expected_hash {
        return Err(SaveError::TransactionVerification(
            "staged save canonical hash differs from source world".to_owned(),
        ));
    }
    preserve_backup(path)?;
    transaction.commit()?;
    verify_committed_world(path, expected_hash)?;
    sync_parent(path)?;
    Ok(())
}

pub fn load_world(path: &Path, max_payload_bytes: usize) -> Result<WorldDocument, SaveError> {
    let bytes = read_bounded(path, max_payload_bytes, HEADER_LENGTH)?;
    decode_world(&bytes, max_payload_bytes)
}

pub fn load_world_recovering(
    path: &Path,
    max_payload_bytes: usize,
) -> Result<RecoveredWorld, SaveError> {
    if let Ok(world) = load_world(path, max_payload_bytes) {
        return Ok(RecoveredWorld {
            world,
            source: LoadSource::Primary,
        });
    }
    let backup = backup_path(path);
    load_world(&backup, max_payload_bytes)
        .map(|world| RecoveredWorld {
            world,
            source: LoadSource::Backup,
        })
        .map_err(|_| SaveError::RecoveryFailed)
}

fn encode_payload(
    schema_version: SchemaVersion,
    generator_version: u32,
    canonical_hash: WorldStateHash,
    payload: &[u8],
) -> Result<Vec<u8>, SaveError> {
    let payload_length = u64::try_from(payload.len()).map_err(|_| SaveError::PayloadTooLarge {
        actual: payload.len(),
        limit: usize::MAX,
    })?;
    let payload_digest = blake3_digest(payload);
    let mut output = Vec::with_capacity(HEADER_LENGTH + payload.len());
    output.extend_from_slice(&MAGIC);
    output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&schema_version.0.to_le_bytes());
    output.extend_from_slice(&generator_version.to_le_bytes());
    output.extend_from_slice(&FLAGS_NONE.to_le_bytes());
    output.extend_from_slice(&payload_length.to_le_bytes());
    output.extend_from_slice(&canonical_hash.0);
    output.extend_from_slice(&payload_digest);
    debug_assert_eq!(output.len(), HEADER_PREFIX_LENGTH);
    output.extend_from_slice(&blake3_digest(&output));
    output.extend_from_slice(payload);
    Ok(output)
}

fn decode_current_world(
    bytes: &[u8],
    max_payload_bytes: usize,
) -> Result<WorldDocument, SaveError> {
    let header = parse_current_header(bytes, max_payload_bytes)?;
    if header.schema_version != CURRENT_WORLD_SCHEMA.0 {
        return Err(SaveError::UnsupportedSchema(header.schema_version));
    }
    let payload = &bytes[HEADER_LENGTH..];
    if blake3_digest(payload) != header.payload_digest {
        return Err(SaveError::PayloadDigestMismatch);
    }
    let (world, trailing): (WorldDocument, _) = postcard::take_from_bytes(payload)?;
    if !trailing.is_empty() {
        return Err(SaveError::InvalidLength);
    }
    world
        .validate()
        .map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    if world.generator_version.0 != header.generator_version {
        return Err(SaveError::InvalidWorld(
            "generator version differs between header and payload".to_owned(),
        ));
    }
    let actual_hash =
        canonical_world_hash(&world).map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    if actual_hash != header.canonical_hash {
        return Err(SaveError::CanonicalHashMismatch);
    }
    Ok(world)
}

fn parse_current_header(bytes: &[u8], max_payload_bytes: usize) -> Result<SaveHeader, SaveError> {
    if bytes.len() < HEADER_LENGTH {
        return Err(SaveError::Truncated);
    }
    let expected_header_digest: [u8; DIGEST_LENGTH] = bytes[HEADER_PREFIX_LENGTH..HEADER_LENGTH]
        .try_into()
        .expect("fixed header digest range");
    if blake3_digest(&bytes[..HEADER_PREFIX_LENGTH]) != expected_header_digest {
        return Err(SaveError::HeaderDigestMismatch);
    }
    let flags = read_u32(bytes, 20);
    if flags != FLAGS_NONE {
        return Err(SaveError::UnsupportedFlags(flags));
    }
    let payload_length = read_u64(bytes, 24);
    let payload_length_usize = bounded_payload_length(payload_length, max_payload_bytes)?;
    let expected_total = HEADER_LENGTH
        .checked_add(payload_length_usize)
        .ok_or(SaveError::InvalidLength)?;
    if bytes.len() != expected_total {
        return Err(SaveError::InvalidLength);
    }
    Ok(SaveHeader {
        schema_version: read_u32(bytes, 12),
        generator_version: read_u32(bytes, 16),
        flags,
        canonical_hash: WorldStateHash(bytes[32..64].try_into().expect("fixed hash range")),
        payload_digest: bytes[64..96]
            .try_into()
            .expect("fixed payload digest range"),
    })
}

fn bounded_payload_length(value: u64, limit: usize) -> Result<usize, SaveError> {
    let length = usize::try_from(value).map_err(|_| SaveError::PayloadTooLarge {
        actual: usize::MAX,
        limit,
    })?;
    if length > limit {
        return Err(SaveError::PayloadTooLarge {
            actual: length,
            limit,
        });
    }
    Ok(length)
}

fn read_bounded(
    path: &Path,
    max_payload_bytes: usize,
    maximum_header_length: usize,
) -> Result<Vec<u8>, SaveError> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let maximum_file_length = u64::try_from(max_payload_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(u64::try_from(maximum_header_length).expect("header length fits u64"));
    if metadata.len() > maximum_file_length {
        return Err(SaveError::PayloadTooLarge {
            actual: usize::try_from(metadata.len()).unwrap_or(usize::MAX),
            limit: max_payload_bytes,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn transaction_bytes(transaction: &mut AtomicWriteFile) -> Result<Vec<u8>, SaveError> {
    transaction.flush()?;
    transaction.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    transaction.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn verify_committed_world(path: &Path, expected_hash: WorldStateHash) -> Result<(), SaveError> {
    let world = load_world(path, DEFAULT_MAX_PAYLOAD_BYTES)?;
    let actual_hash =
        canonical_world_hash(&world).map_err(|error| SaveError::InvalidWorld(error.to_string()))?;
    if actual_hash != expected_hash {
        return Err(SaveError::TransactionVerification(
            "committed save canonical hash differs from source world".to_owned(),
        ));
    }
    Ok(())
}

fn preserve_backup(path: &Path) -> Result<(), SaveError> {
    if !path.exists() {
        return Ok(());
    }
    let backup = backup_path(path);
    if load_world(path, DEFAULT_MAX_PAYLOAD_BYTES).is_err() {
        load_world(&backup, DEFAULT_MAX_PAYLOAD_BYTES)?;
        return Ok(());
    }
    let bytes = read_bounded(path, DEFAULT_MAX_PAYLOAD_BYTES, HEADER_LENGTH)?;
    let mut writer = AtomicWriteFile::open(&backup)?;
    writer.write_all(&bytes)?;
    writer.sync_all()?;
    writer.commit()?;
    load_world(&backup, DEFAULT_MAX_PAYLOAD_BYTES)?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| "world".into(), std::ffi::OsStr::to_os_string);
    name.push(".bak");
    path.with_file_name(name)
}

fn non_empty_parent(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), SaveError> {
    if let Some(parent) = non_empty_parent(path) {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn sync_parent(_path: &Path) -> Result<(), SaveError> {
    Ok(())
}

fn blake3_digest(bytes: &[u8]) -> [u8; DIGEST_LENGTH] {
    *blake3::hash(bytes).as_bytes()
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + size_of::<u32>()]
            .try_into()
            .expect("fixed u32 range"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + size_of::<u64>()]
            .try_into()
            .expect("fixed u64 range"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use worldrunner_world_generation::{WorldGenerationRequest, generate_world};

    #[test]
    fn world_round_trips_through_save_envelope() {
        let world = world(42);
        let encoded = encode_world(&world).unwrap();
        assert_eq!(read_u32(&encoded, 8), FORMAT_VERSION);
        assert_eq!(read_u32(&encoded, 12), CURRENT_WORLD_SCHEMA.0);
        let decoded = decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES).unwrap();
        assert_eq!(decoded, world);
    }

    #[test]
    fn instance_visibility_round_trips_and_cannot_be_omitted() {
        let mut world = world(42);
        world.environment_instances.truncate(2);
        assert_eq!(world.environment_instances.len(), 2);
        world.environment_instances[0].visible = false;
        world.environment_instances[1].visible = true;
        let encoded = encode_world(&world).unwrap();
        assert_eq!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            world
        );
        let instance = &world.environment_instances[0];
        let mut payload = postcard::to_allocvec(instance).unwrap();
        payload.pop();
        assert!(
            postcard::from_bytes::<worldrunner_world_schema::EnvironmentInstance>(&payload)
                .is_err()
        );
    }

    #[test]
    fn catalog_binding_round_trips_without_fabricated_preferences() {
        let mut world = world(42);
        world.catalog.preferred_art_sets = vec!["application-art".to_owned()];
        world.catalog.preferred_platform_profiles = vec!["application-platform".to_owned()];
        let encoded = encode_world(&world).unwrap();
        assert_eq!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES)
                .unwrap()
                .catalog,
            world.catalog
        );
        let mut payload = postcard::to_allocvec(&world.catalog).unwrap();
        payload.pop();
        assert!(
            postcard::from_bytes::<worldrunner_world_schema::CatalogBinding>(&payload).is_err()
        );
    }

    #[test]
    fn every_header_byte_and_sampled_payload_corruption_fails_closed() {
        let encoded = encode_world(&world(42)).unwrap();
        let payload_step = ((encoded.len() - HEADER_LENGTH) / 32).max(1);
        for offset in (0..HEADER_LENGTH).chain((HEADER_LENGTH..encoded.len()).step_by(payload_step))
        {
            let mut corrupt = encoded.clone();
            corrupt[offset] ^= 1;
            assert!(
                decode_world(&corrupt, DEFAULT_MAX_PAYLOAD_BYTES).is_err(),
                "byte {offset}"
            );
        }
    }

    #[test]
    fn payload_generator_version_must_match_the_header() {
        let world = world(42);
        let payload = postcard::to_allocvec(&world).unwrap();
        let encoded = encode_payload(
            world.schema_version,
            world.generator_version.0 + 1,
            canonical_world_hash(&world).unwrap(),
            &payload,
        )
        .unwrap();
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::InvalidWorld(_))
        ));
    }

    #[test]
    fn payload_corruption_is_rejected() {
        let mut encoded = encode_world(&world(42)).unwrap();
        let last = encoded.len() - 1;
        encoded[last] ^= 0xff;
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::PayloadDigestMismatch)
        ));
    }

    #[test]
    fn truncated_save_is_rejected() {
        let mut encoded = encode_world(&world(42)).unwrap();
        encoded.truncate(HEADER_LENGTH - 1);
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::Truncated)
        ));
    }

    #[test]
    fn payload_limit_is_enforced_before_decode() {
        let encoded = encode_world(&world(42)).unwrap();
        assert!(matches!(
            decode_world(&encoded, 16),
            Err(SaveError::PayloadTooLarge { limit: 16, .. })
        ));
    }

    #[test]
    fn unsupported_future_format_is_rejected() {
        let mut encoded = encode_world(&world(42)).unwrap();
        encoded[8..12].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::UnsupportedVersion(version)) if version == FORMAT_VERSION + 1
        ));
    }

    #[test]
    fn unsupported_future_schema_is_rejected_after_header_validation() {
        let mut encoded = encode_world(&world(42)).unwrap();
        encoded[12..16].copy_from_slice(&(CURRENT_WORLD_SCHEMA.0 + 1).to_le_bytes());
        let digest = blake3_digest(&encoded[..HEADER_PREFIX_LENGTH]);
        encoded[HEADER_PREFIX_LENGTH..HEADER_LENGTH].copy_from_slice(&digest);
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::UnsupportedSchema(version)) if version == CURRENT_WORLD_SCHEMA.0 + 1
        ));
    }

    #[test]
    fn header_corruption_is_rejected() {
        let mut encoded = encode_world(&world(42)).unwrap();
        encoded[16] ^= 1;
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::HeaderDigestMismatch)
        ));
    }

    #[test]
    fn canonical_hash_corruption_is_rejected_even_with_valid_header_digest() {
        let mut encoded = encode_world(&world(42)).unwrap();
        encoded[32] ^= 1;
        let digest = blake3_digest(&encoded[..HEADER_PREFIX_LENGTH]);
        encoded[HEADER_PREFIX_LENGTH..HEADER_LENGTH].copy_from_slice(&digest);
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::CanonicalHashMismatch)
        ));
    }

    #[test]
    fn historical_envelopes_are_not_a_migration_input() {
        for version in [1_u32, 2] {
            let mut encoded = encode_world(&world(42)).unwrap();
            encoded[..8].copy_from_slice(b"RDSAVE01");
            encoded[8..12].copy_from_slice(&version.to_le_bytes());
            assert!(matches!(
                decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
                Err(SaveError::InvalidMagic)
            ));
        }
    }

    #[test]
    fn trailing_payload_is_rejected_even_with_valid_digests() {
        let world = world(42);
        let mut payload = postcard::to_allocvec(&world).unwrap();
        payload.push(0);
        let encoded = encode_payload(
            world.schema_version,
            world.generator_version.0,
            canonical_world_hash(&world).unwrap(),
            &payload,
        )
        .unwrap();
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::InvalidLength)
        ));
    }

    #[test]
    fn future_schema_beyond_current_is_rejected() {
        let world = world(42);
        let mut encoded = encode_world(&world).unwrap();
        let future = CURRENT_WORLD_SCHEMA.0 + 10;
        encoded[12..16].copy_from_slice(&future.to_le_bytes());
        let digest = blake3_digest(&encoded[..HEADER_PREFIX_LENGTH]);
        encoded[HEADER_PREFIX_LENGTH..HEADER_LENGTH].copy_from_slice(&digest);
        assert!(matches!(
            decode_world(&encoded, DEFAULT_MAX_PAYLOAD_BYTES),
            Err(SaveError::UnsupportedSchema(version)) if version == future
        ));
    }

    #[test]
    fn transactional_replace_preserves_previous_world_as_backup() {
        let directory = test_directory("replace");
        let path = directory.join("world.rdsave");
        let first = world(1);
        let second = world(2);
        save_world_transactional(&path, &first).unwrap();
        save_world_transactional(&path, &second).unwrap();
        assert_eq!(
            load_world(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            second
        );
        assert_eq!(
            load_world(&backup_path(&path), DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            first
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_prefers_a_valid_primary() {
        let directory = test_directory("primary-recovery");
        let path = directory.join("world.rdsave");
        let first = world(1);
        let second = world(2);
        save_world_transactional(&path, &first).unwrap();
        save_world_transactional(&path, &second).unwrap();
        let recovered = load_world_recovering(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap();
        assert_eq!(recovered.source, LoadSource::Primary);
        assert_eq!(recovered.world, second);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_uses_backup_when_primary_is_corrupt() {
        let directory = test_directory("recovery");
        let path = directory.join("world.rdsave");
        let first = world(1);
        let second = world(2);
        save_world_transactional(&path, &first).unwrap();
        save_world_transactional(&path, &second).unwrap();
        fs::write(&path, b"corrupt").unwrap();
        let recovered = load_world_recovering(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap();
        assert_eq!(recovered.source, LoadSource::Backup);
        assert_eq!(recovered.world, first);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn valid_backup_is_retained_when_replacing_a_corrupt_primary() {
        let directory = test_directory("replace-corrupt");
        let path = directory.join("world.rdsave");
        let first = world(1);
        let second = world(2);
        let third = world(3);
        save_world_transactional(&path, &first).unwrap();
        save_world_transactional(&path, &second).unwrap();
        fs::write(&path, b"corrupt").unwrap();
        save_world_transactional(&path, &third).unwrap();
        assert_eq!(load_world(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap(), third);
        assert_eq!(
            load_world(&backup_path(&path), DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            first
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_transaction_preparation_leaves_primary_unchanged() {
        let directory = test_directory("failed-prepare");
        let path = directory.join("world.rdsave");
        let original = world(1);
        save_world_transactional(&path, &original).unwrap();
        let mut invalid = world(2);
        invalid.terrain.material_indices.pop();
        assert!(save_world_transactional(&path, &invalid).is_err());
        assert_eq!(
            load_world(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            original
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn abandoned_atomic_write_leaves_primary_unchanged() {
        let directory = test_directory("abandoned-transaction");
        let path = directory.join("world.rdsave");
        let original = world(1);
        save_world_transactional(&path, &original).unwrap();
        {
            let mut transaction = AtomicWriteFile::open(&path).unwrap();
            transaction.write_all(b"interrupted partial save").unwrap();
            transaction.sync_all().unwrap();
        }
        assert_eq!(
            load_world(&path, DEFAULT_MAX_PAYLOAD_BYTES).unwrap(),
            original
        );
        fs::remove_dir_all(directory).unwrap();
    }

    fn world(seed: u64) -> WorldDocument {
        let profile_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generation-profile-v1.json");
        let profile =
            worldrunner_world_generation::GenerationProfile::from_authoring(&profile_path)
                .expect("authored generation profile parses");
        let mut request = WorldGenerationRequest::from_profile(profile, "smoke").unwrap();
        request.seed = worldrunner_core_types::WorldSeed(seed);
        generate_world(&request).unwrap()
    }

    fn test_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "real-disunity-save-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
