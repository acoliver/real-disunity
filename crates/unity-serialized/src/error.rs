use thiserror::Error;

use crate::ClassId;

/// Why a serialized file could not be read. Offsets are bytes from the start of the file.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SerializedError {
    #[error("object {path_id} declares {expected} bytes, but {actual} were supplied")]
    ObjectSizeMismatch {
        path_id: i64,
        expected: u32,
        actual: usize,
    },
    #[error("no object layout is implemented for {0}")]
    UnsupportedClass(ClassId),
    #[error("{needed} bytes are needed at offset {offset}, but the data ends at {len}")]
    Truncated {
        offset: usize,
        needed: usize,
        len: usize,
    },
    #[error("serialized file format version {0} is not supported; versions 19 to 22 are")]
    UnsupportedVersion(u32),
    #[error(
        "the header's metadata size {metadata_size} and data offset {data_offset} do not fit \
         a file of {file_size} bytes"
    )]
    InconsistentHeader {
        metadata_size: u64,
        data_offset: u64,
        file_size: u64,
    },
    #[error("the {field} is {value}, which is negative")]
    Negative { field: &'static str, value: i64 },
    #[error("the compressed block is invalid at byte {offset} of its input: {reason}")]
    InvalidCompressedBlock { offset: usize, reason: &'static str },
    #[error("shader platform {platform}, chunk {chunk} does not decompress: {source}")]
    ShaderChunk {
        platform: usize,
        chunk: usize,
        source: Box<SerializedError>,
    },
    #[error("shader platform {platform}, chunk {chunk}: {reason}")]
    InvalidShaderBlob {
        platform: usize,
        chunk: usize,
        reason: &'static str,
    },
    #[error("byte {value} at offset {offset} is not a boolean")]
    InvalidBool { offset: usize, value: u8 },
    #[error("the string at offset {offset} has no terminating zero byte")]
    UnterminatedString { offset: usize },
    #[error("the string at offset {offset} is not UTF-8")]
    InvalidString { offset: usize },
    #[error(
        "the {what} count {count} at offset {offset} is negative or needs more than the \
         {remaining} bytes left"
    )]
    InvalidCount {
        what: &'static str,
        count: i32,
        offset: usize,
        remaining: usize,
    },
    #[error("object {path_id} names type {index}, but the file declares {types} types")]
    UnknownType {
        path_id: i64,
        index: i32,
        types: usize,
    },
    #[error(
        "object {path_id} has {size} bytes at {start} past the data offset, outside the \
         {file_size}-byte file"
    )]
    ObjectOutOfBounds {
        path_id: i64,
        start: u64,
        size: u32,
        file_size: u64,
    },
    #[error("object {path_id} is a {found}, not a {expected}")]
    WrongClass {
        path_id: i64,
        expected: ClassId,
        found: ClassId,
    },
    #[error(
        "the {class} layout read {read} of the {size} bytes of object {path_id}; the layout does \
         not match this Unity version's data"
    )]
    TrailingBytes {
        path_id: i64,
        class: ClassId,
        read: usize,
        size: u32,
    },
    #[error("object path ID {0} appears more than once")]
    DuplicatePathId(i64),
    #[error(
        "type tree string offset {offset} does not start a string in the {len}-byte string \
         buffer"
    )]
    TypeTreeString { offset: u32, len: usize },
    #[error(
        "the header declares {0} bytes of header and metadata, more than this platform can address"
    )]
    MetadataTooLarge(u64),
}
