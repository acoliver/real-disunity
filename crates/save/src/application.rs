use crate::byte_io::check_size;
use crate::{ByteFile, SaveError, Settings, settings};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

mod envelope;

/// Lowercase ASCII letters, digits, `_` and `-`, with 1..=64 bytes.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SlotId(String);

impl SlotId {
    pub fn new(name: &str) -> Result<Self, SaveError> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        {
            return Err(SaveError::InvalidSlotId);
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Byte limits are UTF-8 lengths. All limits are explicit and positive.
#[derive(Clone, Copy, Debug)]
pub struct StoreLimits {
    pub payload_bytes: usize,
    pub label_bytes: usize,
    pub settings_bytes: usize,
    pub value_bytes: usize,
    pub settings_entries: usize,
}

impl StoreLimits {
    fn validate(self) -> Result<(), SaveError> {
        let values = [
            self.payload_bytes,
            self.label_bytes,
            self.settings_bytes,
            self.value_bytes,
            self.settings_entries,
        ];
        if values.contains(&0)
            || values
                .iter()
                .any(|&v| v > (u32::MAX as usize).saturating_sub(4))
            || self
                .payload_bytes
                .max(self.settings_bytes)
                .checked_add(self.label_bytes)
                .and_then(|v| v.checked_add(envelope::HEADER + 4))
                .is_none()
        {
            return Err(SaveError::InvalidLimits);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotMetadata {
    pub label: String,
    /// Application-owned payload schema version, never interpreted by the engine.
    pub payload_version: u32,
}

#[derive(Debug, PartialEq)]
pub struct SlotContents {
    pub metadata: SlotMetadata,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct SlotInfo {
    pub id: SlotId,
    /// Payload byte count, excluding the store envelope and metadata.
    pub size: u64,
    pub modified: SystemTime,
    pub metadata: SlotMetadata,
}

/// One caller-owned directory, with atomic replacement and no recovery fallback.
#[derive(Debug)]
pub struct ApplicationStore {
    root: PathBuf,
    limits: StoreLimits,
}

impl ApplicationStore {
    pub fn new(root: PathBuf, limits: StoreLimits) -> Result<Self, SaveError> {
        limits.validate()?;
        Ok(Self { root, limits })
    }

    pub fn write_slot(
        &self,
        id: &SlotId,
        payload: &[u8],
        metadata: &SlotMetadata,
    ) -> Result<(), SaveError> {
        self.write(
            &self.slot_path(id),
            payload,
            metadata,
            self.limits.payload_bytes,
        )
    }

    pub fn read_slot(&self, id: &SlotId) -> Result<SlotContents, SaveError> {
        self.read(&self.slot_path(id), self.limits.payload_bytes)
    }

    pub fn delete_slot(&self, id: &SlotId) -> Result<(), SaveError> {
        let path = self.slot_path(id);
        fs::remove_file(&path)?;
        #[cfg(unix)]
        crate::byte_io::sync_parent(&path)?;
        Ok(())
    }

    /// Lexical identifier order; corrupt slots fail the whole listing.
    pub fn list_slots(&self) -> Result<Vec<SlotInfo>, SaveError> {
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut slots = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name
                .to_str()
                .and_then(|s| s.strip_prefix("slot-"))
                .and_then(|s| s.strip_suffix(".rds"))
            else {
                continue;
            };
            let id = SlotId::new(name)?;
            let contents = self.read_slot(&id)?;
            slots.push(SlotInfo {
                id,
                size: contents.payload.len() as u64,
                modified: entry.metadata()?.modified()?,
                metadata: contents.metadata,
            });
        }
        slots.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(slots)
    }

    pub fn write_settings(&self, values: &Settings) -> Result<(), SaveError> {
        let payload = settings::encode(values, self.limits)?;
        let metadata = SlotMetadata {
            label: String::new(),
            payload_version: 1,
        };
        self.write(
            &self.root.join("settings.rds"),
            &payload,
            &metadata,
            self.limits.settings_bytes,
        )
    }

    pub fn read_settings(&self) -> Result<Settings, SaveError> {
        let contents = self.read(&self.root.join("settings.rds"), self.limits.settings_bytes)?;
        if contents.metadata.payload_version != 1 {
            return Err(SaveError::UnsupportedVersion(
                contents.metadata.payload_version,
            ));
        }
        settings::decode(&contents.payload, self.limits)
    }

    fn slot_path(&self, id: &SlotId) -> PathBuf {
        self.root.join(format!("slot-{}.rds", id.as_str()))
    }

    fn file(&self, limit: usize) -> ByteFile {
        ByteFile::new(limit + self.limits.label_bytes + 4 + envelope::HEADER)
    }

    fn write(
        &self,
        path: &Path,
        payload: &[u8],
        metadata: &SlotMetadata,
        limit: usize,
    ) -> Result<(), SaveError> {
        check_size(payload.len(), limit)?;
        check_size(metadata.label.len(), self.limits.label_bytes)?;
        let bytes = envelope::encode(payload, metadata);
        self.file(limit).write(path, &bytes, |staged| {
            envelope::decode(staged, limit, self.limits.label_bytes).map(|_| ())
        })
    }

    fn read(&self, path: &Path, limit: usize) -> Result<SlotContents, SaveError> {
        let bytes = self
            .file(limit)
            .read_with_header(path, envelope::HEADER, |header| {
                envelope::inspect(header, limit, self.limits.label_bytes).map(|_| ())
            })?;
        envelope::decode(&bytes, limit, self.limits.label_bytes)
    }
}
