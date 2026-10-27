use crate::{SerializedError, cursor::Cursor};

/// The oldest and newest serialized file format versions the reader accepts. Version 19 is
/// written by Unity 2019.1 and version 22 by Unity 2022.2 through Unity 6.
pub const MIN_VERSION: u32 = 19;
pub const MAX_VERSION: u32 = 22;

/// Header length for format versions before 22: four 32-bit fields, the endianness byte and
/// three reserved bytes.
const LEGACY_HEADER_LEN: usize = 20;
/// Version 22 appends a 32-bit metadata size and signed 64-bit file size, data offset and
/// reserved fields.
const HEADER_LEN: usize = 48;

/// The fixed header at the start of a serialized file. It is always big-endian; the
/// endianness flag it holds applies to the metadata and object data that follow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u32,
    pub metadata_size: u64,
    pub file_size: u64,
    /// Offset of the object data region; object offsets are relative to it.
    pub data_offset: u64,
    pub big_endian: bool,
}

impl Header {
    /// The number of bytes [`Header::parse`] needs.
    pub const MAX_LEN: usize = HEADER_LEN;

    /// Reads the header from the first bytes of a file.
    ///
    /// # Errors
    /// Fails on truncated input, a format version outside 19 to 22, or sizes that cannot
    /// describe a valid file.
    pub fn parse(bytes: &[u8]) -> Result<Self, SerializedError> {
        let mut cursor = Cursor::new(bytes, 0, true);
        let legacy_metadata = cursor.u32()?;
        let legacy_file_size = cursor.u32()?;
        let version = cursor.u32()?;
        let legacy_data_offset = cursor.u32()?;
        if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
            return Err(SerializedError::UnsupportedVersion(version));
        }
        let big_endian = cursor.bool()?;
        cursor.take(3)?;
        let header = if version >= 22 {
            let metadata_size = u64::from(cursor.u32()?);
            let file_size = non_negative(cursor.i64()?, "file size")?;
            let data_offset = non_negative(cursor.i64()?, "data offset")?;
            cursor.i64()?;
            Self {
                version,
                metadata_size,
                file_size,
                data_offset,
                big_endian,
            }
        } else {
            Self {
                version,
                metadata_size: legacy_metadata.into(),
                file_size: legacy_file_size.into(),
                data_offset: legacy_data_offset.into(),
                big_endian,
            }
        };
        header.validate()
    }

    /// Length of the header for this format version; the metadata starts here.
    #[must_use]
    pub fn header_len(&self) -> usize {
        if self.version >= 22 {
            HEADER_LEN
        } else {
            LEGACY_HEADER_LEN
        }
    }

    /// Bytes from the start of the file to the end of the metadata: the prefix that
    /// [`crate::SerializedFile::parse`] needs. Object data is not required.
    #[must_use]
    pub fn metadata_end(&self) -> u64 {
        self.header_len() as u64 + self.metadata_size
    }

    /// [`Header::metadata_end`] as a slice length.
    ///
    /// # Errors
    /// Fails when the length exceeds the platform's address space.
    pub fn metadata_len(&self) -> Result<usize, SerializedError> {
        usize::try_from(self.metadata_end())
            .map_err(|_| SerializedError::MetadataTooLarge(self.metadata_end()))
    }

    fn validate(self) -> Result<Self, SerializedError> {
        if self.metadata_end() > self.data_offset || self.data_offset > self.file_size {
            return Err(SerializedError::InconsistentHeader {
                metadata_size: self.metadata_size,
                data_offset: self.data_offset,
                file_size: self.file_size,
            });
        }
        Ok(self)
    }
}

pub(crate) fn non_negative(value: i64, field: &'static str) -> Result<u64, SerializedError> {
    u64::try_from(value).map_err(|_| SerializedError::Negative { field, value })
}
