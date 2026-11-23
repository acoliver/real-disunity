//! The batch draw through the ABI: a command buffer draws instances of a stored mesh whose per
//! instance data a game's shader reads from a window of a compute buffer. The call validates its
//! pointers, decodes the names and forwards one record; the world checks the rest.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, DrawBatch, MaterialId, OffscreenWork,
};

/// The fields of a batch draw. `metadata_count` names are at the call's `names` pointer
/// (`names_len` bytes of UTF-8 names joined by zero bytes) with one word each at `words`;
/// `visible_count` instance indexes are at `visible`. `tags_len` bytes at `tags` are the pass
/// tags (`LightMode` values) joined by zero bytes, and a record that has them has pass -1.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityDrawBatch {
    pub mesh: u64,
    pub material: u64,
    pub buffer: u64,
    pub offset: u64,
    pub size: u64,
    pub submesh: u32,
    pub pass: i32,
    pub metadata_count: u32,
    pub names_len: u32,
    pub visible_count: u32,
    pub tags_len: u32,
}

/// The pointers of a call, which `count` elements each must be readable for.
struct Lists<'a> {
    names: &'a [u8],
    words: &'a [u32],
    visible: &'a [u32],
    tags: &'a [u8],
}

/// A slice of `count` elements at `data`, or an empty one for a count of zero.
///
/// # Safety
/// `data` is readable for `count` elements when `count` is not zero.
unsafe fn elements<'a, T>(data: *const T, count: u32) -> Result<&'a [T], AbiError> {
    if count == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err("a batch draw's list pointer is null for a nonzero count".into());
    }
    let length = usize::try_from(count).map_err(|_| AbiError::from("a batch list is too long"))?;
    Ok(unsafe { std::slice::from_raw_parts(data, length) })
}

/// The metadata of the call: each name paired with its word. The names must be as many as
/// `count` says.
fn metadata(lists: &Lists<'_>, count: u32) -> Result<Vec<(String, u32)>, AbiError> {
    if count == 0 && lists.names.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<&[u8]> = lists.names.split(|byte| *byte == 0).collect();
    if names.len() != lists.words.len() {
        return Err("a batch draw needs one metadata word for each name".into());
    }
    names
        .into_iter()
        .zip(lists.words)
        .map(|(name, word)| {
            std::str::from_utf8(name)
                .map(|name| (name.to_owned(), *word))
                .map_err(|_| "metadata names must be UTF-8".into())
        })
        .collect()
}

/// The pass tags of the call: the names in `tags` joined by zero bytes, none for no bytes.
fn pass_tags(tags: &[u8]) -> Result<Vec<String>, AbiError> {
    if tags.is_empty() {
        return Ok(Vec::new());
    }
    tags.split(|byte| *byte == 0)
        .map(|tag| {
            std::str::from_utf8(tag)
                .map(str::to_owned)
                .map_err(|_| "pass tags must be UTF-8".into())
        })
        .collect()
}

/// Records a draw of instances of sub-mesh `submesh` of the stored mesh `mesh`, with the
/// material's `pass` (-1 for every pass), into the bound targets with the current view and
/// projection. The shader reads each instance from the window of `buffer` that starts at `offset`
/// and is `size` bytes long. `names` holds `batch.metadata_count` property names joined by zero
/// bytes (`batch.names_len` bytes), `words` their metadata words and `visible` the indexes of the
/// instances to draw, in order, and `tags` the `batch.tags_len` bytes of pass tags, which a
/// draw of a renderer list's passes names instead of a pass index (the pass must then be -1). A
/// count of zero needs no pointer. An identity nothing stored, a
/// dead material or buffer, a window outside the buffer, a name that is empty or too long and a
/// count that disagrees with the names are refused and record nothing.
///
/// # Safety
/// `batch` is readable for one record. `names` is readable for `batch.names_len` bytes, `words`
/// for `batch.metadata_count` words, `visible` for `batch.visible_count` words and `tags` for
/// `batch.tags_len` bytes, each unless its count is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_offscreen_draw_mesh_batch(
    world: u64,
    batch: *const DisunityDrawBatch,
    names: *const u8,
    words: *const u32,
    visible: *const u32,
    tags: *const u8,
) -> DisunityResult {
    if batch.is_null() {
        return DisunityResult::invalid_argument();
    }
    let batch = unsafe { *batch };
    let lists = unsafe {
        (
            elements(names, batch.names_len),
            elements(words, batch.metadata_count),
            elements(visible, batch.visible_count),
            elements(tags, batch.tags_len),
        )
    };
    mutate(world, |s| {
        let (names, words, visible, tags) = lists;
        let lists = Lists {
            names: names?,
            words: words?,
            visible: visible?,
            tags: tags?,
        };
        let pass_tags = pass_tags(lists.tags)?;
        if !pass_tags.is_empty() && batch.pass != -1 {
            return Err("a batch draw with pass tags has the pass -1".into());
        }
        if !s.meshes.contains_key(&batch.mesh) {
            return Err("a batch draw names a mesh identity nothing stored".into());
        }
        s.native.record_offscreen(OffscreenWork::DrawMeshBatch {
            mesh: batch.mesh,
            material: MaterialId::from_bits(batch.material),
            submesh: batch.submesh,
            pass: batch.pass,
            batch: DrawBatch {
                buffer: ComputeBufferId::from_bits(batch.buffer),
                offset: batch.offset,
                size: batch.size,
                metadata: metadata(&lists, batch.metadata_count)?,
                visible: lists.visible.to_vec(),
                pass_tags,
            },
        })?;
        Ok(())
    })
}
