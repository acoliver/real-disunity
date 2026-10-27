//! Reading and decoding individual objects of a built player, without loading whole files.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use worldrunner_unity_serialized::{
    ClassId, ObjectInfo, SerializedError, SerializedFile,
    classes::{GameObject, MonoBehaviour, MonoScript, RectTransform, Transform},
    object::{Layout, decode},
};

use crate::{PlayerError, read_metadata};

/// One serialized file opened for reading objects.
pub struct OpenFile {
    pub path: PathBuf,
    pub file: SerializedFile,
    handle: File,
}

impl OpenFile {
    /// Reads the file's metadata and keeps it open for object reads.
    ///
    /// # Errors
    /// Fails if the file cannot be read or is not a supported serialized file.
    pub fn open(path: &Path) -> Result<Self, PlayerError> {
        let file = read_metadata(path)?;
        let handle = File::open(path).map_err(|source| PlayerError::Io {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self {
            path: path.to_owned(),
            file,
            handle,
        })
    }

    /// The bytes of one object.
    ///
    /// # Errors
    /// Fails if the object's range cannot be read.
    pub fn bytes(&mut self, object: &ObjectInfo) -> Result<Vec<u8>, PlayerError> {
        let io_error = |source| PlayerError::Io {
            path: self.path.clone(),
            source,
        };
        let len = self.handle.metadata().map_err(io_error)?.len();
        if object
            .offset
            .checked_add(u64::from(object.size))
            .is_none_or(|end| end > len)
        {
            return Err(self.error(SerializedError::ObjectOutOfBounds {
                path_id: object.path_id,
                start: object.offset,
                size: object.size,
                file_size: len,
            }));
        }
        let mut bytes = vec![0; object.size as usize];
        self.handle
            .seek(SeekFrom::Start(object.offset))
            .map_err(io_error)?;
        self.handle.read_exact(&mut bytes).map_err(io_error)?;
        Ok(bytes)
    }

    /// Decodes the object with this path ID using `T`'s layout.
    ///
    /// # Errors
    /// Fails if the object is missing, of another class, or does not decode.
    pub fn decode<T: Layout>(&mut self, path_id: i64) -> Result<T, PlayerError> {
        let object = *self
            .file
            .object(path_id)
            .ok_or_else(|| PlayerError::MissingObject {
                path: self.path.clone(),
                path_id,
            })?;
        let bytes = self.bytes(&object)?;
        decode(&bytes, &object, self.file.header.big_endian).map_err(|source| self.error(source))
    }

    fn error(&self, source: SerializedError) -> PlayerError {
        PlayerError::Serialized {
            path: self.path.clone(),
            source,
        }
    }
}

/// How many objects of one class decoded and failed, with the first failures.
#[derive(Debug, Default)]
pub struct ClassCensus {
    pub decoded: usize,
    pub failed: usize,
    pub examples: Vec<String>,
}

/// Classes [`decode_census`] decodes.
pub const DECODED_CLASSES: [ClassId; 5] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::RECT_TRANSFORM,
    ClassId::MONO_SCRIPT,
    ClassId::MONO_BEHAVIOUR,
];
const FAILURES_KEPT: usize = 5;

/// Decodes every object of the [`DECODED_CLASSES`] in the given files and counts the results.
///
/// # Errors
/// Fails if a file cannot be opened or an object cannot be read; decoding failures are
/// counted, not returned.
pub fn decode_census(paths: &[PathBuf]) -> Result<BTreeMap<ClassId, ClassCensus>, PlayerError> {
    let mut census = BTreeMap::<ClassId, ClassCensus>::new();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let objects: Vec<_> = open.file.objects().to_vec();
        for object in objects
            .iter()
            .filter(|o| DECODED_CLASSES.contains(&o.class_id))
        {
            let bytes = open.bytes(object)?;
            let big_endian = open.file.header.big_endian;
            let entry = census.entry(object.class_id).or_default();
            match decode_any(&bytes, object, big_endian) {
                Ok(()) => entry.decoded += 1,
                Err(error) => {
                    entry.failed += 1;
                    if entry.examples.len() < FAILURES_KEPT {
                        entry.examples.push(format!("{}: {error}", path.display()));
                    }
                }
            }
        }
    }
    Ok(census)
}

fn decode_any(bytes: &[u8], object: &ObjectInfo, big_endian: bool) -> Result<(), SerializedError> {
    match object.class_id {
        ClassId::GAME_OBJECT => decode::<GameObject>(bytes, object, big_endian).map(drop),
        ClassId::TRANSFORM => decode::<Transform>(bytes, object, big_endian).map(drop),
        ClassId::RECT_TRANSFORM => decode::<RectTransform>(bytes, object, big_endian).map(drop),
        ClassId::MONO_SCRIPT => decode::<MonoScript>(bytes, object, big_endian).map(drop),
        ClassId::MONO_BEHAVIOUR => decode::<MonoBehaviour>(bytes, object, big_endian).map(drop),
        class => Err(SerializedError::UnsupportedClass(class)),
    }
}
