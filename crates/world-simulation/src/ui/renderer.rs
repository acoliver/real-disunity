//! The state of a canvas renderer: what a UI element submits to be drawn.
use crate::ui::group::{effective_alpha, groups_by_object};
use crate::{ComponentDesc, ComponentHandle, UiError, World, WorldError};

/// How many materials, and how many pop materials, one renderer holds.
pub const MAX_CANVAS_MATERIALS: usize = 8;

/// The three switches of a renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CanvasRendererFlags {
    /// Whether the renderer's geometry is skipped.
    pub cull: bool,
    /// Whether a fully transparent mesh is skipped.
    pub cull_transparent_mesh: bool,
    /// Whether the element carries a mask pop instruction.
    pub has_pop_instruction: bool,
}

/// What a UI element submits: a colour, an alpha, a mesh, textures, materials and a clip
/// rectangle. Meshes, textures and materials are managed instance ids, 0 for none. Nothing
/// draws this yet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasRenderer {
    pub color: [f32; 4],
    /// Kept apart from the colour's alpha; the engine multiplies them when it draws.
    pub alpha: f32,
    pub flags: CanvasRendererFlags,
    /// The clip rectangle as x, y, width, height, or none when rectangle clipping is off.
    pub clip_rect: Option<[f32; 4]>,
    pub clipping_softness: [f32; 2],
    pub material_count: u32,
    pub pop_material_count: u32,
    pub mesh: u64,
    pub texture: u64,
    pub alpha_texture: u64,
    /// Slots at or past `material_count` are always 0.
    pub materials: [u64; MAX_CANVAS_MATERIALS],
    /// Slots at or past `pop_material_count` are always 0.
    pub pop_materials: [u64; MAX_CANVAS_MATERIALS],
}

impl Default for CanvasRenderer {
    fn default() -> Self {
        Self {
            color: [1.; 4],
            alpha: 1.,
            flags: CanvasRendererFlags::default(),
            clip_rect: None,
            clipping_softness: [0.; 2],
            material_count: 0,
            pop_material_count: 0,
            mesh: 0,
            texture: 0,
            alpha_texture: 0,
            materials: [0; MAX_CANVAS_MATERIALS],
            pop_materials: [0; MAX_CANVAS_MATERIALS],
        }
    }
}

/// The alpha the renderer's element is drawn with: its own alpha times the alpha of every
/// enabled canvas group on its object and the objects above, ending at the first group that
/// ignores the groups above it.
pub fn inherited_alpha(world: &World, renderer: ComponentHandle) -> Result<f32, WorldError> {
    let ComponentDesc::CanvasRenderer(desc) = world.component(renderer)?.desc else {
        return Err(WorldError::from(UiError::InvalidCanvasRenderer));
    };
    let object = world.component(renderer)?.object;
    let groups = groups_by_object(&world.snapshot().components);
    Ok(desc.alpha * effective_alpha(world, &groups, object)?)
}

impl CanvasRenderer {
    /// Clears the material slots past the counts. A NaN alpha or a count above
    /// [`MAX_CANVAS_MATERIALS`] is rejected. Any other alpha is kept as given (the engine's
    /// choice), as are colour, clip rectangle and softness.
    pub fn normalized(mut self) -> Result<Self, UiError> {
        if self.alpha.is_nan() {
            return Err(UiError::InvalidCanvasRenderer);
        }
        let (count, pops) = (slots(self.material_count)?, slots(self.pop_material_count)?);
        self.materials[count..].fill(0);
        self.pop_materials[pops..].fill(0);
        Ok(self)
    }

    pub(crate) fn validate(self) -> Result<(), UiError> {
        let normal = self.normalized()?;
        if normal.materials == self.materials && normal.pop_materials == self.pop_materials {
            Ok(())
        } else {
            Err(UiError::InvalidCanvasRenderer)
        }
    }

    /// Sets a main material (`pop` false) or a pop material (`pop` true). The slot must be
    /// inside the matching count.
    pub fn with_material(mut self, pop: bool, index: u32, id: u64) -> Result<Self, UiError> {
        let (count, list) = if pop {
            (self.pop_material_count, &mut self.pop_materials)
        } else {
            (self.material_count, &mut self.materials)
        };
        if index >= count {
            return Err(UiError::InvalidCanvasRenderer);
        }
        list[index as usize] = id;
        Ok(self)
    }
}

fn slots(count: u32) -> Result<usize, UiError> {
    let count = count as usize;
    if count > MAX_CANVAS_MATERIALS {
        return Err(UiError::InvalidCanvasRenderer);
    }
    Ok(count)
}
