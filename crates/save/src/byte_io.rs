use crate::SaveError;
use atomic_write_file::AtomicWriteFile;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Bounded file reads and atomic, verified replacement. No backup is created.
#[derive(Clone, Copy, Debug)]
pub struct ByteFile {
    maximum_bytes: usize,
}

impl ByteFile {
    pub const fn new(maximum_bytes: usize) -> Self {
        Self { maximum_bytes }
    }

    pub fn read(&self, path: &Path) -> Result<Vec<u8>, SaveError> {
        self.read_with_header(path, 0, |_| Ok(()))
    }

    /// Inspect a fixed-size header before allocating or reading the body.
    pub fn read_with_header(
        &self,
        path: &Path,
        header_bytes: usize,
        inspect: impl FnOnce(&[u8]) -> Result<(), SaveError>,
    ) -> Result<Vec<u8>, SaveError> {
        check_size(header_bytes, self.maximum_bytes)?;
        let mut file = File::open(path)?;
        let mut header = vec![0; header_bytes];
        file.read_exact(&mut header).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                SaveError::Truncated
            } else {
                SaveError::Io(error)
            }
        })?;
        inspect(&header)?;
        let length = usize::try_from(file.metadata()?.len()).unwrap_or(usize::MAX);
        check_size(length, self.maximum_bytes)?;
        let remaining = self.maximum_bytes - header_bytes;
        file.take((remaining as u64).saturating_add(1))
            .read_to_end(&mut header)?;
        check_size(header.len(), self.maximum_bytes)?;
        Ok(header)
    }

    /// Verification runs after staging and syncing, before the destination changes.
    pub fn write(
        &self,
        path: &Path,
        bytes: &[u8],
        verify: impl FnOnce(&[u8]) -> Result<(), SaveError>,
    ) -> Result<(), SaveError> {
        check_size(bytes.len(), self.maximum_bytes)?;
        if let Some(parent) = non_empty_parent(path) {
            fs::create_dir_all(parent)?;
        }
        let mut options = AtomicWriteFile::options();
        options.read(true);
        let mut transaction = options.open(path)?;
        transaction.write_all(bytes)?;
        transaction.sync_all()?;
        transaction.seek(SeekFrom::Start(0))?;
        let mut staged = Vec::with_capacity(bytes.len());
        (&mut transaction)
            .take((self.maximum_bytes as u64).saturating_add(1))
            .read_to_end(&mut staged)?;
        check_size(staged.len(), self.maximum_bytes)?;
        verify(&staged)?;
        transaction.commit()?;
        #[cfg(unix)]
        sync_parent(path)?;
        Ok(())
    }
}

pub(crate) fn check_size(actual: usize, limit: usize) -> Result<(), SaveError> {
    if actual > limit {
        return Err(SaveError::PayloadTooLarge { actual, limit });
    }
    Ok(())
}

fn non_empty_parent(path: &Path) -> Option<&Path> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
}

#[cfg(unix)]
pub(crate) fn sync_parent(path: &Path) -> Result<(), SaveError> {
    if let Some(parent) = non_empty_parent(path) {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_replacement_preserves_previous_bytes() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/save-tests")
            .join(format!("interrupted-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("bytes");
        fs::write(&path, b"previous").unwrap();
        let result = ByteFile::new(32).write(&path, b"next", |_| {
            Err(SaveError::TransactionVerification(
                "simulated interruption".into(),
            ))
        });
        assert!(matches!(result, Err(SaveError::TransactionVerification(_))));
        assert_eq!(fs::read(&path).unwrap(), b"previous");
        fs::remove_dir_all(directory).unwrap();
    }
}
