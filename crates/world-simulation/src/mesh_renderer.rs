//! Renderer components of 3D objects: the settings of a mesh renderer or skinned mesh renderer,
//! and the mesh a mesh filter names. Meshes are managed instance ids; materials, property blocks
//! and bones stay in the managed component, which keeps a material count here and, per material
//! slot, the colour and texture the host draws the slot with. The presentation lists them as
//! mesh objects; nothing draws them yet.
use crate::{ComponentError, WorldError};

/// Most materials one renderer holds.
pub const MAX_RENDERER_MATERIALS: u32 = 64;

/// Which renderer class the component is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RendererKind {
    #[default]
    Mesh,
    Skinned,
    /// The renderer of a particle system. It holds the common renderer settings; the particle
    /// specific ones stay in the managed component.
    Particle,
}

/// The three drawing switches of a renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RendererSwitches {
    pub enabled: bool,
    /// Skips drawing without changing `enabled`.
    pub force_rendering_off: bool,
    pub receive_shadows: bool,
}

impl Default for RendererSwitches {
    fn default() -> Self {
        Self {
            enabled: true,
            force_rendering_off: false,
            receive_shadows: true,
        }
    }
}

/// What the host draws one material slot of a mesh renderer with: a colour multiplier as the material holds it
/// and the renderer identity of a texture, 0 for none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnMaterial {
    pub color: [f32; 4],
    pub texture: u64,
}

impl Default for DrawnMaterial {
    /// A slot with no information draws white and untextured.
    fn default() -> Self {
        Self {
            color: [1.0; 4],
            texture: 0,
        }
    }
}

/// The settings of a mesh renderer or a skinned mesh renderer.
///
/// The mode fields hold the numeric value of the matching script enum: shadow casting 0 to 3,
/// motion vectors 0 to 2, light probe usage 0, 1, 2 or 4, reflection probe usage 0 to 3. Sorting
/// layer 0 is the only layer the engine registers.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshRenderer {
    pub kind: RendererKind,
    pub switches: RendererSwitches,
    pub allow_occlusion_when_dynamic: bool,
    pub static_shadow_caster: bool,
    pub shadow_casting_mode: u32,
    pub motion_vector_mode: u32,
    pub light_probe_usage: u32,
    pub reflection_probe_usage: u32,
    pub rendering_layer_mask: u32,
    pub renderer_priority: i32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    /// How many materials the managed component holds, 0 through [`MAX_RENDERER_MATERIALS`].
    pub material_count: u32,
    /// What the host draws each material slot with, at most `material_count` of them; a slot
    /// past the end draws as [`DrawnMaterial::default`]. Only the managed component's material
    /// update sets them, and lowering `material_count` drops the slots past it.
    pub drawn: Vec<DrawnMaterial>,
}

impl Default for MeshRenderer {
    /// A new renderer is enabled, casts and receives shadows, blends probes, draws on layer 1
    /// and has no material.
    fn default() -> Self {
        Self {
            kind: RendererKind::Mesh,
            switches: RendererSwitches::default(),
            allow_occlusion_when_dynamic: true,
            static_shadow_caster: false,
            shadow_casting_mode: 1,
            motion_vector_mode: 1,
            light_probe_usage: 1,
            reflection_probe_usage: 1,
            rendering_layer_mask: 1,
            renderer_priority: 0,
            sorting_layer: 0,
            sorting_order: 0,
            material_count: 0,
            drawn: Vec::new(),
        }
    }
}

impl MeshRenderer {
    /// The defaults of a renderer of `kind`.
    pub fn of_kind(kind: RendererKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        let valid = self.shadow_casting_mode <= 3
            && self.motion_vector_mode <= 2
            && matches!(self.light_probe_usage, 0 | 1 | 2 | 4)
            && self.reflection_probe_usage <= 3
            && self.material_count <= MAX_RENDERER_MATERIALS
            && self.drawn.len() <= self.material_count as usize
            && self.drawn.iter().all(|slot| {
                slot.color
                    .iter()
                    .all(|channel| channel.is_finite() && *channel >= 0.0)
            });
        if valid {
            Ok(())
        } else {
            Err(WorldError::InvalidComponent(
                ComponentError::InvalidMeshRenderer,
            ))
        }
    }
}

/// The mesh a mesh filter names, as a managed instance id, 0 for none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshFilter {
    pub mesh: u64,
    /// The renderer identity the host knows the mesh's contents by, 0 when the host has none. Only
    /// the managed component's mesh update sets it, and naming another mesh resets it.
    pub drawn_mesh: u64,
}
