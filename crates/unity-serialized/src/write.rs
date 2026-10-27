//! Writes serialized files: the inverse of [`crate::SerializedFile::parse`]. Tools and tests use it
//! to produce small files in the player format.

use std::{collections::BTreeMap, sync::Arc};

use thiserror::Error;

use crate::{
    External, MAX_VERSION, MIN_VERSION, NodeString, ScriptRef, SerializedType, TypeTreeNode,
    file::has_script_id,
};

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum WriteError {
    #[error("serialized file format version {0} is not supported; versions 19 to 22 are")]
    UnsupportedVersion(u32),
    #[error("the {0} does not fit the format's field size")]
    TooLarge(&'static str),
    #[error("reference type {0} names no script class; type trees need one")]
    MissingScriptClass(usize),
    #[error(
        "type {index} {}; the format stores one for every MonoBehaviour type and for \
         reference types that name a script",
        if *has { "has a script ID it cannot store" } else { "lacks the script ID it needs" }
    )]
    ScriptId { index: usize, has: bool },
}

/// One object to write: its path ID, the index of its type and its serialized bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectData {
    pub path_id: i64,
    pub type_index: i32,
    pub data: Vec<u8>,
}

/// Everything a serialized file holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileContents {
    pub version: u32,
    pub big_endian: bool,
    pub unity_version: String,
    pub target_platform: i32,
    pub type_trees: bool,
    pub types: Vec<SerializedType>,
    pub objects: Vec<ObjectData>,
    pub scripts: Vec<ScriptRef>,
    pub externals: Vec<External>,
    /// Written for format version 20 and later.
    pub ref_types: Vec<SerializedType>,
    pub user_information: String,
}

/// Object data starts on a 16-byte boundary and each object on an 8-byte one, as Unity
/// lays them out.
const DATA_ALIGN: usize = 16;
const OBJECT_ALIGN: usize = 8;

impl FileContents {
    /// Encodes the file.
    ///
    /// # Errors
    /// Fails on a format version outside 19 to 22, on sizes or counts that overflow the
    /// format's fields, and on reference types without a script class when type trees are on.
    pub fn write(&self) -> Result<Vec<u8>, WriteError> {
        if !(MIN_VERSION..=MAX_VERSION).contains(&self.version) {
            return Err(WriteError::UnsupportedVersion(self.version));
        }
        let header_len = if self.version >= 22 { 48 } else { 20 };
        let (starts, data_len) = self.data_layout();
        let mut w = Writer::new(self.big_endian, header_len);
        w.string(&self.unity_version);
        w.i32(self.target_platform);
        w.u8(u8::from(self.type_trees));
        self.write_types(&mut w, &self.types, false)?;
        self.write_objects(&mut w, &starts)?;
        w.count(self.scripts.len(), "script count")?;
        for script in &self.scripts {
            w.i32(script.file_index);
            w.align4();
            w.i64(script.path_id);
        }
        w.count(self.externals.len(), "external count")?;
        for external in &self.externals {
            w.string("");
            w.bytes(&external.guid);
            w.i32(external.kind);
            w.string(&external.path);
        }
        if self.version >= 20 {
            self.write_types(&mut w, &self.ref_types, true)?;
        }
        w.string(&self.user_information);
        self.finish(w.buf, header_len, &starts, data_len)
    }

    fn data_layout(&self) -> (Vec<usize>, usize) {
        let mut at = 0;
        let starts = self
            .objects
            .iter()
            .map(|object| {
                let start = at;
                at = (at + object.data.len()).next_multiple_of(OBJECT_ALIGN);
                start
            })
            .collect();
        (starts, at)
    }

    fn write_objects(&self, w: &mut Writer, starts: &[usize]) -> Result<(), WriteError> {
        w.count(self.objects.len(), "object count")?;
        for (object, &start) in self.objects.iter().zip(starts) {
            w.align4();
            w.i64(object.path_id);
            if self.version >= 22 {
                w.u64(to_u64(start)?);
            } else {
                w.u32(fit(start, "object offset")?);
            }
            w.u32(fit(object.data.len(), "object size")?);
            w.i32(object.type_index);
        }
        Ok(())
    }

    fn write_types(
        &self,
        w: &mut Writer,
        types: &[SerializedType],
        reference: bool,
    ) -> Result<(), WriteError> {
        w.count(types.len(), "type count")?;
        for (index, ty) in types.iter().enumerate() {
            w.i32(ty.class_id.0);
            w.u8(u8::from(ty.stripped));
            w.i16(ty.script_type_index);
            let expected = has_script_id(ty.class_id, ty.script_type_index, reference);
            match (ty.script_id, expected) {
                (Some(id), true) => w.bytes(&id),
                (None, false) => {}
                (id, _) => {
                    return Err(WriteError::ScriptId {
                        index,
                        has: id.is_some(),
                    });
                }
            }
            w.bytes(&ty.old_type_hash);
            if self.type_trees {
                write_tree(w, ty.type_tree.as_deref().unwrap_or_default())?;
                if self.version >= 21 {
                    write_type_links(w, ty, reference, index)?;
                }
            }
        }
        Ok(())
    }

    fn finish(
        &self,
        mut buf: Vec<u8>,
        header_len: usize,
        starts: &[usize],
        data_len: usize,
    ) -> Result<Vec<u8>, WriteError> {
        let metadata_size = fit(buf.len() - header_len, "metadata size")?;
        let data_offset = buf.len().next_multiple_of(DATA_ALIGN);
        buf.resize(data_offset + data_len, 0);
        for (object, start) in self.objects.iter().zip(starts) {
            let at = data_offset + start;
            buf[at..at + object.data.len()].copy_from_slice(&object.data);
        }
        let mut h = Writer::new(true, 0);
        if self.version >= 22 {
            h.bytes(&[0; 8]);
            h.u32(self.version);
            h.bytes(&[0; 4]);
            h.u8(u8::from(self.big_endian));
            h.bytes(&[0; 3]);
            h.u32(metadata_size);
            h.u64(to_u64(buf.len())?);
            h.u64(to_u64(data_offset)?);
            h.u64(0);
        } else {
            h.u32(metadata_size);
            h.u32(fit(buf.len(), "file size")?);
            h.u32(self.version);
            h.u32(fit(data_offset, "data offset")?);
            h.u8(u8::from(self.big_endian));
            h.bytes(&[0; 3]);
        }
        buf[..h.buf.len()].copy_from_slice(&h.buf);
        Ok(buf)
    }
}

fn write_type_links(
    w: &mut Writer,
    ty: &SerializedType,
    reference: bool,
    index: usize,
) -> Result<(), WriteError> {
    if reference {
        let class = ty
            .script_class
            .as_ref()
            .ok_or(WriteError::MissingScriptClass(index))?;
        w.string(&class.class_name);
        w.string(&class.namespace);
        w.string(&class.assembly);
    } else {
        w.count(ty.dependencies.len(), "type dependency count")?;
        ty.dependencies.iter().for_each(|&d| w.i32(d));
    }
    Ok(())
}

fn write_tree(w: &mut Writer, nodes: &[TypeTreeNode]) -> Result<(), WriteError> {
    // Each distinct name is stored once, as Unity does.
    let mut strings = Vec::new();
    let mut stored = BTreeMap::<Arc<str>, u32>::new();
    let mut offset_of = |s: &NodeString| match s {
        NodeString::Common(offset) => Ok(offset | 1 << 31),
        NodeString::Local(text) => {
            if let Some(&at) = stored.get(&**text) {
                return Ok(at);
            }
            let at = fit(strings.len(), "type tree string buffer")?;
            strings.extend_from_slice(text.as_bytes());
            strings.push(0);
            stored.insert(Arc::clone(text), at);
            Ok(at)
        }
    };
    let offsets = nodes
        .iter()
        .map(|n| Ok((offset_of(&n.type_name)?, offset_of(&n.name)?)))
        .collect::<Result<Vec<_>, WriteError>>()?;
    w.count(nodes.len(), "type tree node count")?;
    w.count(strings.len(), "type tree string buffer")?;
    for (node, (type_offset, name_offset)) in nodes.iter().zip(offsets) {
        w.u16(node.version);
        w.u8(node.level);
        w.u8(node.type_flags);
        w.u32(type_offset);
        w.u32(name_offset);
        w.i32(node.byte_size);
        w.i32(node.index);
        w.u32(node.meta_flags);
        w.u64(node.ref_type_hash);
    }
    w.bytes(&strings);
    Ok(())
}

fn fit<T: TryFrom<usize>>(value: usize, what: &'static str) -> Result<T, WriteError> {
    T::try_from(value).map_err(|_| WriteError::TooLarge(what))
}

fn to_u64(value: usize) -> Result<u64, WriteError> {
    fit(value, "file offset")
}

struct Writer {
    big_endian: bool,
    buf: Vec<u8>,
}

macro_rules! put {
    ($($name:ident: $ty:ty),*) => {
        $(fn $name(&mut self, value: $ty) {
            let bytes = if self.big_endian { value.to_be_bytes() } else { value.to_le_bytes() };
            self.buf.extend_from_slice(&bytes);
        })*
    };
}

impl Writer {
    fn new(big_endian: bool, start: usize) -> Self {
        Self {
            big_endian,
            buf: vec![0; start],
        }
    }

    put!(u8: u8, u16: u16, i16: i16, u32: u32, i32: i32, u64: u64, i64: i64);

    fn bytes(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn string(&mut self, text: &str) {
        self.bytes(text.as_bytes());
        self.buf.push(0);
    }

    fn count(&mut self, count: usize, what: &'static str) -> Result<(), WriteError> {
        self.i32(fit(count, what)?);
        Ok(())
    }

    fn align4(&mut self) {
        let len = self.buf.len().next_multiple_of(4);
        self.buf.resize(len, 0);
    }
}

/// Writes one object's fields, the inverse of [`crate::object::ObjectReader`]. Alignment is
/// relative to the start of the object.
pub struct ObjectWriter {
    inner: Writer,
}

impl ObjectWriter {
    #[must_use]
    pub fn new(big_endian: bool) -> Self {
        Self {
            inner: Writer::new(big_endian, 0),
        }
    }

    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.inner.buf
    }

    pub fn u8(&mut self, value: u8) -> &mut Self {
        self.inner.u8(value);
        self
    }

    pub fn u16(&mut self, value: u16) -> &mut Self {
        self.inner.u16(value);
        self
    }

    pub fn u32(&mut self, value: u32) -> &mut Self {
        self.inner.u32(value);
        self
    }

    pub fn i32(&mut self, value: i32) -> &mut Self {
        self.inner.i32(value);
        self
    }

    pub fn f32(&mut self, value: f32) -> &mut Self {
        self.inner.u32(value.to_bits());
        self
    }

    pub fn floats(&mut self, values: &[f32]) -> &mut Self {
        values.iter().for_each(|&v| self.inner.u32(v.to_bits()));
        self
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.inner.bytes(bytes);
        self
    }

    pub fn align(&mut self) -> &mut Self {
        self.inner.align4();
        self
    }

    /// # Errors
    /// Fails when the string is longer than the format's 32-bit length.
    pub fn string(&mut self, text: &str) -> Result<&mut Self, WriteError> {
        self.inner.count(text.len(), "string length")?;
        self.inner.bytes(text.as_bytes());
        self.inner.align4();
        Ok(self)
    }

    pub fn pptr(&mut self, pptr: crate::object::PPtr) -> &mut Self {
        self.inner.i32(pptr.file_id);
        self.inner.i64(pptr.path_id);
        self
    }

    /// # Errors
    /// Fails when there are more references than the format's 32-bit count.
    pub fn pptrs(&mut self, pptrs: &[crate::object::PPtr]) -> Result<&mut Self, WriteError> {
        self.inner.count(pptrs.len(), "reference count")?;
        for &pptr in pptrs {
            self.pptr(pptr);
        }
        self.inner.align4();
        Ok(self)
    }
}
