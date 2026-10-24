use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{MAX_IMAGE_BYTES, capture::error};

pub(super) fn validate(relative: &Path, expected: &str, maximum: u64) -> Result<(), String> {
    if maximum == 0 || maximum > MAX_IMAGE_BYTES {
        return Err("image maximum must be between 1 and 64 MiB".into());
    }
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected image SHA-256 must be 64 lowercase hex digits".into());
    }
    let mut normal = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => normal.push(part),
            _ => return Err("image path must be normal and relative".into()),
        }
    }
    if normal.as_os_str().is_empty() || normal.as_os_str() != relative.as_os_str() {
        return Err("image path must be normal and relative".into());
    }
    Ok(())
}

pub(super) fn verify(file: &mut File, length: u64, expected: &str) -> Result<(), String> {
    file.seek(SeekFrom::Start(0)).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut remaining = length;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64)).map_err(error)?;
        let read = file.read(&mut buffer[..count]).map_err(error)?;
        if read == 0 {
            return Err("sealed image length mismatch".into());
        }
        hash.update(&buffer[..read]);
        remaining -= read as u64;
    }
    if file.read(&mut buffer[..1]).map_err(error)? != 0
        || format!("{:x}", hash.finalize()) != expected
    {
        return Err("image SHA-256 mismatch".into());
    }
    Ok(())
}
