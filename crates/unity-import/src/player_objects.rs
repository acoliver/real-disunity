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
    graphics::{Material, MeshFilter, MeshRenderer, Texture2D},
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

/// Decodes an object with one class's layout and describes the result.
type Check = fn(&[u8], &ObjectInfo, bool) -> Result<String, SerializedError>;

fn check<T: Layout + std::fmt::Debug>(
    bytes: &[u8],
    object: &ObjectInfo,
    big_endian: bool,
) -> Result<String, SerializedError> {
    decode::<T>(bytes, object, big_endian).map(|value| format!("{value:#?}"))
}

/// Decodes one object with its class's layout and describes its fields.
///
/// # Errors
/// Fails if the object is missing, cannot be read, has no layout, or does not decode.
pub fn describe_object(open: &mut OpenFile, path_id: i64) -> Result<String, PlayerError> {
    let object = *open
        .file
        .object(path_id)
        .ok_or_else(|| PlayerError::MissingObject {
            path: open.path.clone(),
            path_id,
        })?;
    let check = layout_check(object.class_id)
        .ok_or_else(|| open.error(SerializedError::UnsupportedClass(object.class_id)))?;
    let bytes = open.bytes(&object)?;
    check(&bytes, &object, open.file.header.big_endian).map_err(|source| open.error(source))
}

/// The classes with layouts, each with the check [`decode_census`] runs on its objects.
const LAYOUTS: [(ClassId, Check); 9] = [
    (ClassId::GAME_OBJECT, check::<GameObject>),
    (ClassId::TRANSFORM, check::<Transform>),
    (ClassId::RECT_TRANSFORM, check::<RectTransform>),
    (ClassId::MONO_SCRIPT, check::<MonoScript>),
    (ClassId::MONO_BEHAVIOUR, check::<MonoBehaviour>),
    (ClassId::MESH_FILTER, check::<MeshFilter>),
    (ClassId::MESH_RENDERER, check::<MeshRenderer>),
    (ClassId::MATERIAL, check::<Material>),
    (ClassId::TEXTURE_2D, check::<Texture2D>),
];
const FAILURES_KEPT: usize = 5;

/// The classes [`decode_census`] decodes.
pub fn decoded_classes() -> impl Iterator<Item = ClassId> {
    LAYOUTS.iter().map(|&(class, _)| class)
}

fn layout_check(class: ClassId) -> Option<Check> {
    LAYOUTS
        .iter()
        .find(|&&(c, _)| c == class)
        .map(|&(_, check)| check)
}

/// Decodes every object of the [`decoded_classes`] in the given files and counts the results.
///
/// # Errors
/// Fails if a file cannot be opened or an object cannot be read; decoding failures are
/// counted, not returned.
pub fn decode_census(paths: &[PathBuf]) -> Result<BTreeMap<ClassId, ClassCensus>, PlayerError> {
    let mut census = BTreeMap::<ClassId, ClassCensus>::new();
    for path in paths {
        let mut open = OpenFile::open(path)?;
        let objects: Vec<_> = open.file.objects().to_vec();
        for object in &objects {
            let Some(check) = layout_check(object.class_id) else {
                continue;
            };
            let bytes = open.bytes(object)?;
            let big_endian = open.file.header.big_endian;
            let entry = census.entry(object.class_id).or_default();
            match check(&bytes, object, big_endian) {
                Ok(_) => entry.decoded += 1,
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
