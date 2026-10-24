use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

use rustix::fs::{self, MemfdFlags, Mode, OFlags, ResolveFlags, SealFlags};

use super::{SealedImage, identity};

pub(super) fn capture(
    root: &Path,
    relative: &Path,
    expected: &str,
    maximum: u64,
) -> Result<SealedImage, String> {
    identity::validate(relative, expected, maximum)?;
    let mut source = open_source(root, relative)?;
    let length = bounded_length(&source, maximum)?;
    let mut file = File::from(
        fs::memfd_create(
            "compat-image",
            MemfdFlags::ALLOW_SEALING | MemfdFlags::CLOEXEC,
        )
        .map_err(error)?,
    );
    copy_exact(&mut source, &mut file, length)?;
    if bounded_length(&source, maximum)? != length {
        return Err("image length changed during capture".into());
    }
    identity::verify(&mut file, length, expected)?;
    let required = SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL;
    fs::fcntl_add_seals(&file, required).map_err(error)?;
    if !fs::fcntl_get_seals(&file)
        .map_err(error)?
        .contains(required)
    {
        return Err("image seals incomplete".into());
    }
    identity::verify(&mut file, length, expected)?;
    Ok(SealedImage {
        file,
        digest: expected.into(),
        length,
        maximum,
    })
}

fn open_source(root: &Path, relative: &Path) -> Result<File, String> {
    let canonical = root.canonicalize().map_err(error)?;
    let directory = fs::open(
        &canonical,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(error)?;
    fs::openat2(
        &directory,
        relative,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
        ResolveFlags::BENEATH | ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )
    .map(File::from)
    .map_err(error)
}

fn bounded_length(file: &File, maximum: u64) -> Result<u64, String> {
    let metadata = file.metadata().map_err(error)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err("image must be a regular nonempty file within declared maximum".into());
    }
    Ok(metadata.len())
}

fn copy_exact(source: &mut File, destination: &mut File, length: u64) -> Result<(), String> {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut remaining = length;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(buffer.len() as u64)).map_err(error)?;
        let read = source.read(&mut buffer[..count]).map_err(error)?;
        if read == 0 {
            return Err("image truncated during capture".into());
        }
        destination.write_all(&buffer[..read]).map_err(error)?;
        remaining -= read as u64;
    }
    if source.read(&mut buffer[..1]).map_err(error)? != 0 {
        return Err("image grew during capture".into());
    }
    destination.seek(SeekFrom::Start(0)).map_err(error)?;
    Ok(())
}

pub(super) fn error(error: impl std::fmt::Display) -> String {
    format!("image capture: {error}")
}
