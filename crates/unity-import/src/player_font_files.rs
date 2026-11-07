//! Where a player font's font file lies in its serialized file.

use std::path::{Path, PathBuf};

use worldrunner_unity_serialized::font::Font;

use crate::{PlayerError, player_files::PlayerFiles};

/// Where a font's font file lies: `length` bytes at `offset` of the player file `file`.
pub(crate) struct FontRange {
    pub(crate) name: String,
    pub(crate) file: PathBuf,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}

/// The font file range of the font `path_id` of `file`.
pub(crate) fn font_range(
    files: &mut PlayerFiles,
    file: &Path,
    path_id: i64,
) -> Result<FontRange, PlayerError> {
    let open = files.open_path(file)?;
    let font: Font = open.decode(path_id)?;
    let object = *open.file.object(path_id).expect("decoded objects exist");
    Ok(FontRange {
        name: font.name,
        file: std::path::absolute(file).map_err(|error| PlayerError::Io {
            path: file.to_owned(),
            source: error,
        })?,
        offset: object.offset + font.font_data.start as u64,
        length: (font.font_data.end - font.font_data.start) as u64,
    })
}
