use super::{SlotContents, SlotMetadata};
use crate::{
    SaveError, blake3_digest, bounded_payload_length, byte_io::check_size, read_u32, read_u64,
};

const MAGIC: &[u8; 8] = b"RDAPP001";
pub(super) const HEADER: usize = 88;

pub(super) fn encode(payload: &[u8], metadata: &SlotMetadata) -> Vec<u8> {
    let metadata_length = metadata.label.len() + 4;
    let mut body = Vec::with_capacity(metadata_length + payload.len());
    body.extend_from_slice(&metadata.payload_version.to_le_bytes());
    body.extend_from_slice(metadata.label.as_bytes());
    body.extend_from_slice(payload);
    let mut bytes = Vec::with_capacity(HEADER + body.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(metadata_length)
            .expect("validated metadata length")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&blake3_digest(&body));
    bytes.extend_from_slice(&blake3_digest(&bytes));
    bytes.extend_from_slice(&body);
    bytes
}

pub(super) fn inspect(
    bytes: &[u8],
    limit: usize,
    label_limit: usize,
) -> Result<(usize, usize), SaveError> {
    if bytes.len() < HEADER {
        return Err(SaveError::Truncated);
    }
    if &bytes[..8] != MAGIC {
        return Err(SaveError::InvalidMagic);
    }
    let version = read_u32(bytes, 8);
    if version != 1 {
        return Err(SaveError::UnsupportedVersion(version));
    }
    if blake3_digest(&bytes[..56]) != bytes[56..HEADER] {
        return Err(SaveError::HeaderDigestMismatch);
    }
    let metadata = read_u32(bytes, 12) as usize;
    if metadata < 4 {
        return Err(SaveError::InvalidLength);
    }
    check_size(metadata - 4, label_limit)?;
    let payload = bounded_payload_length(read_u64(bytes, 16), limit)?;
    Ok((metadata, payload))
}

pub(super) fn decode(
    bytes: &[u8],
    limit: usize,
    label_limit: usize,
) -> Result<SlotContents, SaveError> {
    let (metadata, payload) = inspect(bytes, limit, label_limit)?;
    let expected = HEADER
        .checked_add(metadata)
        .and_then(|v| v.checked_add(payload))
        .ok_or(SaveError::InvalidLength)?;
    if bytes.len() != expected {
        return Err(SaveError::InvalidLength);
    }
    let body = &bytes[HEADER..];
    if blake3_digest(body) != bytes[24..56] {
        return Err(SaveError::PayloadDigestMismatch);
    }
    let label = std::str::from_utf8(&body[4..metadata]).map_err(|_| SaveError::InvalidMetadata)?;
    Ok(SlotContents {
        metadata: SlotMetadata {
            label: label.to_owned(),
            payload_version: read_u32(body, 0),
        },
        payload: body[metadata..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_oversize_is_rejected_from_header_alone() {
        let metadata = SlotMetadata {
            label: String::new(),
            payload_version: 0,
        };
        let mut bytes = encode(b"", &metadata);
        bytes.truncate(HEADER);
        bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
        let digest = blake3_digest(&bytes[..56]);
        bytes[56..HEADER].copy_from_slice(&digest);
        assert!(matches!(
            inspect(&bytes, 1024, 32),
            Err(SaveError::PayloadTooLarge { .. })
        ));
        bytes[8..12].copy_from_slice(&99_u32.to_le_bytes());
        assert!(matches!(
            inspect(&bytes, 1024, 32),
            Err(SaveError::UnsupportedVersion(99))
        ));
    }
}
