//! Declares the integer coordinate vectors naga's SPIR-V frontend expects for image access.
//!
//! naga splits an image coordinate into the part its image dimension needs and an array
//! layer, and looks up a vector of the coordinate's scalar kind at that size: two components
//! for a 2D image, three for 3D and cube images. For float coordinates naga declares the
//! vector itself; for integer coordinates it panics when the module does not declare one,
//! as with an `OpImageFetch` of a 2D array image at an `ivec3` in a module without `ivec2`.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    BOUND_WORD, HEADER_WORDS, Ids, OP_TYPE_INT, OP_TYPE_VECTOR, PrepareError, instructions, opcode,
    result_types,
};

const OP_TYPE_IMAGE: u32 = 25;
const OP_IMAGE_FETCH: u32 = 95;
const OP_IMAGE_READ: u32 = 98;
const OP_IMAGE_WRITE: u32 = 99;

/// The image and coordinate operands of an image access with integer coordinates.
fn image_and_coordinate(instruction: &[u32]) -> Option<(u32, u32)> {
    match opcode(instruction) {
        OP_IMAGE_FETCH | OP_IMAGE_READ => Some((*instruction.get(3)?, *instruction.get(4)?)),
        OP_IMAGE_WRITE => Some((*instruction.get(1)?, *instruction.get(2)?)),
        _ => None,
    }
}

/// The coordinate size naga requires for each image type.
fn image_sizes(words: &[u32]) -> BTreeMap<u32, u32> {
    instructions(words)
        .filter(|instruction| opcode(instruction) == OP_TYPE_IMAGE)
        .filter_map(|instruction| {
            let size = match *instruction.get(3)? {
                1 => 2,
                2 | 3 => 3,
                _ => return None,
            };
            Some((instruction[1], size))
        })
        .collect()
}

/// The 32-bit integer scalar and size of each coordinate vector that an integer image access
/// needs and the module does not declare.
fn missing_coordinate_types(words: &[u32]) -> BTreeSet<(u32, u32)> {
    let mut int32 = BTreeSet::new();
    let mut vectors = BTreeMap::new();
    for instruction in instructions(words) {
        match (opcode(instruction), instruction) {
            (OP_TYPE_INT, &[_, id, 32, _]) => {
                int32.insert(id);
            }
            (OP_TYPE_VECTOR, &[_, id, component, count]) => {
                vectors.insert(id, (component, count));
            }
            _ => {}
        }
    }
    let declared: BTreeSet<(u32, u32)> = vectors.values().copied().collect();
    let (sizes, types) = (image_sizes(words), result_types(words));
    instructions(words)
        .filter_map(image_and_coordinate)
        .filter_map(|(image, coordinate)| {
            let size = *sizes.get(types.get(&image)?)?;
            let coordinate_type = *types.get(&coordinate)?;
            let component = vectors
                .get(&coordinate_type)
                .map_or(coordinate_type, |&(component, _)| component);
            int32.contains(&component).then_some((component, size))
        })
        .filter(|needed| !declared.contains(needed))
        .collect()
}

/// Declares each coordinate vector that an integer image access needs and the module lacks,
/// right after its component type.
pub(super) fn image_coordinate_types(words: &[u32]) -> Result<Vec<u32>, PrepareError> {
    let missing = missing_coordinate_types(words);
    if missing.is_empty() {
        return Ok(words.to_vec());
    }
    let mut ids = Ids {
        bound: words[BOUND_WORD],
    };
    let mut out = words[..HEADER_WORDS].to_vec();
    for instruction in instructions(words) {
        out.extend_from_slice(instruction);
        let Some(&id) = instruction
            .get(1)
            .filter(|_| opcode(instruction) == OP_TYPE_INT)
        else {
            continue;
        };
        for &(component, size) in missing.range((id, 0)..=(id, u32::MAX)) {
            out.extend([(4 << 16) | OP_TYPE_VECTOR, ids.next()?, component, size]);
        }
    }
    out[BOUND_WORD] = ids.bound;
    Ok(out)
}
