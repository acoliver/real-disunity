//! Material pipeline keys and GPU uniform encoding.
use wgpu::Face;
use worldrunner_render_plan::{SceneAlphaMode, SceneMaterial};

use crate::shader;

/// Divisor turning governed micro units into the floats the shader reads.
const MICRO_SCALE: f32 = 1_000_000.0;

/// Size in bytes of the shader's `Material` uniform block.
pub const MATERIAL_UNIFORM_BYTES: usize = 432;

/// The fixed-function and shader-program state one material requires.
///
/// This is deliberately small and `Copy`: it is a map key, looked up once per
/// draw, and it names only the properties that cannot vary within a pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PipelineKey {
    pub relief: bool,
    /// Whether the fragment shader discards texels below the alpha cutoff.
    pub masked: bool,
    /// Whether back faces are drawn rather than culled.
    pub double_sided: bool,
}

impl PipelineKey {
    /// Every pipeline this renderer can require, in a stable order.
    ///
    /// The stable indices address the shared material-variant cache. Variants
    /// compile on first use so unused relief shaders do not delay ordinary scenes.
    pub const ALL: [Self; 8] = [
        Self {
            masked: false,
            double_sided: false,
            relief: false,
        },
        Self {
            masked: false,
            double_sided: true,
            relief: false,
        },
        Self {
            masked: true,
            double_sided: false,
            relief: false,
        },
        Self {
            masked: true,
            double_sided: true,
            relief: false,
        },
        Self {
            masked: false,
            double_sided: false,
            relief: true,
        },
        Self {
            masked: false,
            double_sided: true,
            relief: true,
        },
        Self {
            masked: true,
            double_sided: false,
            relief: true,
        },
        Self {
            masked: true,
            double_sided: true,
            relief: true,
        },
    ];

    /// Derives the pipeline state one governed material requires.
    #[must_use]
    pub fn from_material(material: &SceneMaterial) -> Self {
        Self {
            relief: material.impostor().is_some_and(|a| a.relief().is_some()),
            masked: matches!(material.alpha_mode(), SceneAlphaMode::Mask { .. }),
            double_sided: material.double_sided(),
        }
    }

    /// Returns this key's position in [`PipelineKey::ALL`].
    ///
    /// Callers hold their pipelines in an array built by mapping over `ALL`, so
    /// this arithmetic and that order have to agree. A test pins them together:
    /// were they to drift, every masked material would silently draw through
    /// the opaque pipeline, which discards nothing, and cutout foliage would
    /// render as opaque rectangles.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.relief) * 4 + usize::from(self.masked) * 2 + usize::from(self.double_sided)
    }

    #[must_use]
    pub const fn cull_mode(self) -> Option<Face> {
        if self.double_sided {
            None
        } else {
            Some(Face::Back)
        }
    }

    /// Returns whether the shader must flip normals on back-facing fragments.
    ///
    /// A double-sided surface presents its geometric back to the camera half
    /// the time, and its authored normal points the wrong way there. Left
    /// uncorrected, the lit and unlit sides of every leaf swap depending on
    /// view direction, which reads as foliage that flickers between bright and
    /// black as the camera orbits.
    #[must_use]
    pub const fn flips_back_face_normals(self) -> bool {
        self.double_sided
    }
}

/// The per-material numbers the shader reads, as distinct from pipeline state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialUniform {
    pub impostor: Option<worldrunner_render_plan::SceneImpostor>,
    pub normal_encoding: worldrunner_render_plan::SceneNormalEncoding,
    pub normal_y: worldrunner_render_plan::SceneNormalY,
    pub normal_scale_micro: u32,
    pub translucency_micro: u32,
    pub terrain_contact: Option<worldrunner_render_plan::SceneTerrainContact>,
    /// Alpha below which a masked fragment is discarded, in micro units.
    ///
    /// Zero for unmasked materials, where the shader never performs the test.
    pub alpha_cutoff_micro: u32,
    /// Roughness multiplier applied to the sampled map, in micro units.
    pub roughness_micro: u32,
    /// Metalness multiplier applied to the sampled map, in micro units.
    pub metalness_micro: u32,
    /// Which optional texture channels this material actually binds.
    ///
    /// The shader cannot ask whether a binding is real, and an unbound slot
    /// samples as opaque black: an absent ambient-occlusion map would darken
    /// the surface to nothing rather than leave it unoccluded. So presence is
    /// stated here instead of inferred there.
    pub texture_flags: u32,
    /// Optional scene material inputs encoded after the stock uniform prefix.
    pub features: Option<worldrunner_render_plan::SceneMaterialFeatures>,
}

impl MaterialUniform {
    /// Extracts the numeric material parameters from a governed material.
    #[must_use]
    pub fn from_material(material: &SceneMaterial) -> Self {
        let alpha_cutoff_micro = match material.alpha_mode() {
            SceneAlphaMode::Mask { cutoff_micro } => cutoff_micro,
            SceneAlphaMode::Opaque => 0,
        };
        let mut texture_flags = 0;
        if material.normal().is_some() {
            texture_flags |= shader::FLAG_NORMAL;
        }
        if material.roughness_metallic().is_some() {
            texture_flags |= shader::FLAG_ROUGH_METAL;
        }
        if material.ambient_occlusion().is_some() {
            texture_flags |= shader::FLAG_AMBIENT_OCCLUSION;
        }
        if material.self_illumination().is_some() {
            texture_flags |= shader::FLAG_SELF_ILLUMINATION;
        }
        Self {
            impostor: material.impostor(),
            normal_encoding: material.normal_encoding(),
            normal_y: material.normal_y(),
            normal_scale_micro: material.normal_scale_micro(),
            translucency_micro: material.translucency_micro(),
            terrain_contact: material.terrain_contact(),
            alpha_cutoff_micro,
            roughness_micro: material.roughness_micro(),
            metalness_micro: material.metalness_micro(),
            texture_flags,
            features: material.features().copied(),
        }
    }

    /// Encodes the uniform in the layout the shader's `Material` struct declares.
    ///
    /// Hand-encoded because the workspace forbids `unsafe`, so there is no
    /// casting a struct to bytes. The scalars become floats here rather than in
    /// the shader so that the micro-unit convention stops at this boundary.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "governed micro units are validated to 0..=1_000_000, well inside f32's exact integer range"
    )]
    pub fn to_bytes(self) -> [u8; MATERIAL_UNIFORM_BYTES] {
        let mut bytes = [0u8; MATERIAL_UNIFORM_BYTES];
        let alpha_cutoff = self.alpha_cutoff_micro as f32 / MICRO_SCALE;
        let roughness = self.roughness_micro as f32 / MICRO_SCALE;
        let metalness = self.metalness_micro as f32 / MICRO_SCALE;
        bytes[0..4].copy_from_slice(&alpha_cutoff.to_le_bytes());
        bytes[4..8].copy_from_slice(&roughness.to_le_bytes());
        bytes[8..12].copy_from_slice(&metalness.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.texture_flags.to_le_bytes());
        features::encode(&mut bytes, self.features.as_ref());
        features::encode_impostor(&mut bytes, self.impostor);
        features::encode_surface(&mut bytes, self.translucency_micro, self.terrain_contact);
        let scale = self.normal_scale_micro as f32 / MICRO_SCALE;
        bytes[72..76].copy_from_slice(&scale.to_le_bytes());
        let encoding =
            u32::from(self.normal_encoding == worldrunner_render_plan::SceneNormalEncoding::Rgb)
                | (u32::from(self.normal_y == worldrunner_render_plan::SceneNormalY::Down) << 1);
        bytes[76..80].copy_from_slice(&encoding.to_le_bytes());
        bytes
    }
}

pub(crate) mod features;

#[cfg(test)]
mod tests;
