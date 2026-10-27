use std::collections::BTreeMap;

use crate::{
    ClassId, Header, SerializedError, TypeTreeNode, cursor::Cursor, header::non_negative, type_tree,
};

/// Smallest encodings of each metadata entry, used to reject counts that cannot fit.
const MIN_TYPE: usize = 23;
const MIN_OBJECT: usize = 20;
const MIN_SCRIPT: usize = 12;
const MIN_EXTERNAL: usize = 22;
const HASH_LEN: usize = 16;

/// A type the file's objects use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedType {
    pub class_id: ClassId,
    pub stripped: bool,
    /// Index into [`SerializedFile::scripts`] for script types, or -1.
    pub script_type_index: i16,
    /// Hash of the script's serialized layout, present for `MonoBehaviour` types.
    pub script_id: Option<[u8; HASH_LEN]>,
    pub old_type_hash: [u8; HASH_LEN],
    /// The field layout, when the file carries type trees.
    pub type_tree: Option<Vec<TypeTreeNode>>,
    /// Indices of reference types this type depends on, when the file carries type trees.
    pub dependencies: Vec<i32>,
    /// The managed class of a reference type, when the file carries type trees.
    pub script_class: Option<ScriptClass>,
}

/// The managed class behind a `SerializeReference` type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptClass {
    pub class_name: String,
    pub namespace: String,
    pub assembly: String,
}

/// Where one object's data lies in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub path_id: i64,
    /// Absolute offset of the object's data from the start of the file.
    pub offset: u64,
    pub size: u32,
    /// Index into [`SerializedFile::types`].
    pub type_index: usize,
    pub class_id: ClassId,
}

/// A script class referenced by the file: an object in this file or an external one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptRef {
    /// 0 for this file, otherwise one more than an index into [`SerializedFile::externals`].
    pub file_index: i32,
    pub path_id: i64,
}

/// Another file this file's references point into. A reference with file ID `n > 0`
/// names `externals[n - 1]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub guid: [u8; HASH_LEN],
    pub kind: i32,
    pub path: String,
}

/// The metadata of a serialized file: its types, objects, scripts and external files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerializedFile {
    pub header: Header,
    pub unity_version: String,
    pub target_platform: i32,
    pub type_trees: bool,
    pub scripts: Vec<ScriptRef>,
    pub externals: Vec<External>,
    pub ref_types: Vec<SerializedType>,
    pub user_information: String,
    types: Vec<SerializedType>,
    objects: Vec<ObjectInfo>,
    by_path_id: BTreeMap<i64, usize>,
}

impl SerializedFile {
    /// Reads the header and metadata. `bytes` needs to hold the file from its start through
    /// [`Header::metadata_end`]; object data beyond that is neither needed nor read.
    ///
    /// # Errors
    /// Fails on an unsupported format version, on truncated or malformed metadata, and on
    /// objects that name an unknown type, repeat a path ID, or lie outside the file.
    pub fn parse(bytes: &[u8]) -> Result<Self, SerializedError> {
        let header = Header::parse(bytes)?;
        let end = header.metadata_len()?;
        let metadata = bytes.get(..end).ok_or_else(|| SerializedError::Truncated {
            offset: bytes.len(),
            needed: end - bytes.len(),
            len: bytes.len(),
        })?;
        let mut cursor = Cursor::new(metadata, header.header_len(), header.big_endian);
        let unity_version = cursor.string()?;
        let target_platform = cursor.i32()?;
        let type_trees = cursor.bool()?;
        let types = read_types(&mut cursor, &header, type_trees, false)?;
        let objects = read_objects(&mut cursor, &header, &types)?;
        let scripts = read_scripts(&mut cursor)?;
        let externals = read_externals(&mut cursor)?;
        let ref_types = if header.version >= 20 {
            read_types(&mut cursor, &header, type_trees, true)?
        } else {
            Vec::new()
        };
        let user_information = cursor.string()?;
        let by_path_id = index(&objects)?;
        Ok(Self {
            header,
            unity_version,
            target_platform,
            type_trees,
            scripts,
            externals,
            ref_types,
            user_information,
            types,
            objects,
            by_path_id,
        })
    }

    /// Objects in the order the file lists them.
    #[must_use]
    pub fn objects(&self) -> &[ObjectInfo] {
        &self.objects
    }

    #[must_use]
    pub fn object(&self, path_id: i64) -> Option<&ObjectInfo> {
        self.by_path_id.get(&path_id).map(|&i| &self.objects[i])
    }

    /// The types objects use, indexed by [`ObjectInfo::type_index`].
    #[must_use]
    pub fn types(&self) -> &[SerializedType] {
        &self.types
    }

    /// The type of the object with this path ID.
    #[must_use]
    pub fn object_type(&self, path_id: i64) -> Option<&SerializedType> {
        self.object(path_id)
            .map(|object| &self.types[object.type_index])
    }
}

fn read_types(
    cursor: &mut Cursor<'_>,
    header: &Header,
    type_trees: bool,
    reference: bool,
) -> Result<Vec<SerializedType>, SerializedError> {
    let what = if reference { "reference type" } else { "type" };
    let count = cursor.count(what, MIN_TYPE)?;
    (0..count)
        .map(|_| read_type(cursor, header, type_trees, reference))
        .collect()
}

fn read_type(
    cursor: &mut Cursor<'_>,
    header: &Header,
    type_trees: bool,
    reference: bool,
) -> Result<SerializedType, SerializedError> {
    let class_id = ClassId(cursor.i32()?);
    let stripped = cursor.bool()?;
    let script_type_index = cursor.i16()?;
    let script_id = has_script_id(class_id, script_type_index, reference)
        .then(|| cursor.array())
        .transpose()?;
    let old_type_hash = cursor.array()?;
    let mut ty = SerializedType {
        class_id,
        stripped,
        script_type_index,
        script_id,
        old_type_hash,
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    };
    if type_trees {
        ty.type_tree = Some(type_tree::read(cursor)?);
        if header.version >= 21 {
            read_type_links(cursor, &mut ty, reference)?;
        }
    }
    Ok(ty)
}

/// Whether a type entry carries a script ID: every `MonoBehaviour` type, and reference types
/// that name a script.
pub(crate) fn has_script_id(class_id: ClassId, script_type_index: i16, reference: bool) -> bool {
    (reference && script_type_index >= 0) || class_id == ClassId::MONO_BEHAVIOUR
}

fn read_type_links(
    cursor: &mut Cursor<'_>,
    ty: &mut SerializedType,
    reference: bool,
) -> Result<(), SerializedError> {
    if reference {
        ty.script_class = Some(ScriptClass {
            class_name: cursor.string()?,
            namespace: cursor.string()?,
            assembly: cursor.string()?,
        });
    } else {
        let count = cursor.count("type dependency", 4)?;
        ty.dependencies = (0..count).map(|_| cursor.i32()).collect::<Result<_, _>>()?;
    }
    Ok(())
}

fn read_objects(
    cursor: &mut Cursor<'_>,
    header: &Header,
    types: &[SerializedType],
) -> Result<Vec<ObjectInfo>, SerializedError> {
    let count = cursor.count("object", MIN_OBJECT)?;
    (0..count)
        .map(|_| read_object(cursor, header, types))
        .collect()
}

fn read_object(
    cursor: &mut Cursor<'_>,
    header: &Header,
    types: &[SerializedType],
) -> Result<ObjectInfo, SerializedError> {
    cursor.align4()?;
    let path_id = cursor.i64()?;
    let start = if header.version >= 22 {
        non_negative(cursor.i64()?, "object offset")?
    } else {
        cursor.u32()?.into()
    };
    let size = cursor.u32()?;
    let index = cursor.i32()?;
    let type_index = usize::try_from(index)
        .ok()
        .filter(|&i| i < types.len())
        .ok_or(SerializedError::UnknownType {
            path_id,
            index,
            types: types.len(),
        })?;
    let out_of_bounds = SerializedError::ObjectOutOfBounds {
        path_id,
        start,
        size,
        file_size: header.file_size,
    };
    let offset = header
        .data_offset
        .checked_add(start)
        .filter(|offset| {
            offset
                .checked_add(size.into())
                .is_some_and(|end| end <= header.file_size)
        })
        .ok_or(out_of_bounds)?;
    Ok(ObjectInfo {
        path_id,
        offset,
        size,
        type_index,
        class_id: types[type_index].class_id,
    })
}

fn read_scripts(cursor: &mut Cursor<'_>) -> Result<Vec<ScriptRef>, SerializedError> {
    let count = cursor.count("script", MIN_SCRIPT)?;
    (0..count)
        .map(|_| {
            let file_index = cursor.i32()?;
            cursor.align4()?;
            let path_id = cursor.i64()?;
            Ok(ScriptRef {
                file_index,
                path_id,
            })
        })
        .collect()
}

fn read_externals(cursor: &mut Cursor<'_>) -> Result<Vec<External>, SerializedError> {
    let count = cursor.count("external file", MIN_EXTERNAL)?;
    (0..count)
        .map(|_| {
            // An editor-only asset path, empty in builds.
            cursor.string()?;
            Ok(External {
                guid: cursor.array()?,
                kind: cursor.i32()?,
                path: cursor.string()?,
            })
        })
        .collect()
}

fn index(objects: &[ObjectInfo]) -> Result<BTreeMap<i64, usize>, SerializedError> {
    let mut map = BTreeMap::new();
    for (i, object) in objects.iter().enumerate() {
        if map.insert(object.path_id, i).is_some() {
            return Err(SerializedError::DuplicatePathId(object.path_id));
        }
    }
    Ok(map)
}
