//! What a shadow caster reads of a material, and which materials it reads alike.
//!
//! A caster writes depth alone. Its vertex stage reads the wind, impostor and fade blocks of the
//! material and its fragment stage the fade policy, and a masked caster also reads the cutoff and
//! the albedo. The shading numbers, the tint and occlusion, the lighting record and the surface
//! channels belong to the forward stages. Two unmasked materials that agree on every byte a
//! caster reads and lend it the same resources therefore cast the same depth, and the casters bind
//! the first of them for both.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{ImportTable, ImportedResource};

use super::program::semantic;
use crate::pipeline::MATERIAL_UNIFORM_BYTES;

/// The bytes of the material block that no caster stage reads: roughness, metalness, texture
/// flags and tint with occlusion, the terrain contact and the fold sampling that only place the
/// forward stages' ground fold, the geometry flags and lighting record, and the surface channels.
/// A masked caster also reads the cutoff at the start of the block, which is why only unmasked
/// draws are bound as another material.
const FORWARD_ONLY: [Range<usize>; 3] = [0..32, 60..160, 448..MATERIAL_UNIFORM_BYTES];

/// The semantics an unmasked caster binds besides the material block: the sampler of the wind
/// field, the contact heights that fade tables share, and the vertex stage's impostor borders and
/// wind field. The textures a surface is shaded with are the fragment stages' of the forward pass
/// and masked casters.
const BOUND: [&str; 4] = [
    semantic::SAMPLER,
    semantic::CONTACT_HEIGHTS,
    semantic::IMPOSTOR_BORDERS,
    semantic::WIND_FIELD,
];

/// The bytes of `uniform` that a caster reads, with the others cleared.
pub(super) fn read(uniform: &[u8; MATERIAL_UNIFORM_BYTES]) -> Box<[u8]> {
    let mut read = uniform.to_vec();
    for range in FORWARD_ONLY {
        read[range].fill(0);
    }
    read.into()
}

fn resources(table: &ImportTable) -> [Option<&ImportedResource>; BOUND.len()] {
    BOUND.map(|name| table.get(&EngineSemantic::named(name)))
}

/// For each material, the first material that an unmasked caster reads as it does: the one with
/// the same read bytes and the same bound resources. A material that is the first of its kind is
/// its own.
pub(super) fn classes(reads: &[Box<[u8]>], tables: &[Arc<ImportTable>]) -> Vec<usize> {
    let mut seen: HashMap<&[u8], Vec<usize>> = HashMap::new();
    (0..reads.len())
        .map(|material| {
            let alike = seen.entry(&reads[material]).or_default();
            let first = alike
                .iter()
                .copied()
                .find(|&first| resources(&tables[first]) == resources(&tables[material]));
            first.unwrap_or_else(|| {
                alike.push(material);
                material
            })
        })
        .collect()
}
