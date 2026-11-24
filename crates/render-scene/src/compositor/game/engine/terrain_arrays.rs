//! Terrain material arrays: the resident textures that scene terrain surfaces name, gathered into
//! one 2D array per channel for a game's pass to index by layer.
use super::{EngineConstants, SceneSources, fill::Fill};
use crate::compositor::game::lent::FrameInputError;
use crate::compositor::game::resources::{GameResourceError, GameResources};
use crate::compositor::game::scene_textures::FrameSceneTextureError;
use worldrunner_render_gpu::census::{ResourceClass, Tracked, texture_backing_bytes};
use worldrunner_render_plan::{SceneTerrain, TerrainLayer, TextureHandle};
use worldrunner_render_post::PrepareContext;
use worldrunner_render_program::binding::EngineSemantic;
use worldrunner_render_program::runtime::{ExternalHold, Import};

/// Which of a terrain surface's three layer maps an array gathers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainMaterialChannel {
    /// `TerrainLayer::albedo_metallic`: albedo in RGB and metallic in alpha, by default.
    AlbedoMetallic,
    /// `TerrainLayer::normal_roughness`: tangent normal in RGB and roughness in alpha, by default.
    NormalRoughness,
    /// `TerrainLayer::height`: height in the red channel, by default.
    Height,
}

/// The format, extent and mip count the layers of one array share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerShape {
    pub format: wgpu::TextureFormat,
    pub extent: [u32; 2],
    pub mips: u32,
}

/// Why a terrain material array cannot be lent.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TerrainMaterialArrayError {
    #[error(transparent)]
    Input(#[from] FrameInputError),
    #[error("the scene has no terrain")]
    NoTerrain,
    #[error("an array needs at least one layer")]
    NoLayers,
    /// The array's bytes cannot be charged to the factory's resource budget.
    #[error(transparent)]
    Budget(#[from] GameResourceError),
    #[error("{requested} layers exceed the {limit} the device allows in one array")]
    TooManyLayers { requested: usize, limit: u32 },
    #[error("terrain surface type {raw_type} is not among the {surfaces} of the scene's terrain")]
    UnknownType { raw_type: u8, surfaces: usize },
    #[error("terrain surface type {raw_type} has no layered appearance, so no {channel:?} map")]
    NoChannel {
        raw_type: u8,
        channel: TerrainMaterialChannel,
    },
    #[error(transparent)]
    Texture(#[from] FrameSceneTextureError),
    #[error("scene texture {texture} is not a single-layer 2D texture that can be copied")]
    NotCopyable { texture: u32 },
    #[error("layer {layer} is {found:?} where layer 0 is {expected:?}")]
    Disagree {
        layer: usize,
        expected: LayerShape,
        found: LayerShape,
    },
}

const ARRAY_LABEL: &str = "engine terrain material array";

/// One array kept for a channel and layer list.
pub(super) struct MaterialArray {
    pub(super) channel: TerrainMaterialChannel,
    pub(super) layers: Vec<u8>,
    pub(super) texture: Tracked<wgpu::Texture>,
    pub(super) view: wgpu::TextureView,
    /// The bytes each factory that asked for the array is charged.
    bytes: u64,
    /// One hold for each factory (by `GameResources::id`) that asked for the array, in its
    /// runtime ledger. The array's bytes are counted there until the array is dropped.
    holds: Vec<(u64, ExternalHold)>,
    /// The frame that stages the copy of every layer.
    copies: Fill,
}

impl MaterialArray {
    /// Counts the array in `charge`'s ledger and budget, once for each factory.
    fn charge_to(&mut self, charge: &GameResources) -> Result<(), TerrainMaterialArrayError> {
        if self.holds.iter().all(|(id, _)| *id != charge.id()) {
            let hold = charge.admit(ARRAY_LABEL, self.bytes)?;
            self.holds.push((charge.id(), hold));
        }
        Ok(())
    }
}

impl LayerShape {
    fn of(texture: &wgpu::Texture) -> Self {
        Self {
            format: texture.format(),
            extent: [texture.width(), texture.height()],
            mips: texture.mip_level_count(),
        }
    }
}

impl TerrainMaterialChannel {
    fn handle(self, layer: TerrainLayer) -> TextureHandle {
        match self {
            Self::AlbedoMetallic => layer.albedo_metallic,
            Self::NormalRoughness => layer.normal_roughness,
            Self::Height => layer.height,
        }
    }
}

/// The resident textures of `layers`, one for each, and the shape they all share.
fn resolve<'a>(
    sources: &'a SceneSources,
    terrain: &SceneTerrain,
    (channel, layers): (TerrainMaterialChannel, &[u8]),
) -> Result<(Vec<&'a wgpu::Texture>, LayerShape), TerrainMaterialArrayError> {
    let mut textures: Vec<&wgpu::Texture> = Vec::with_capacity(layers.len());
    let mut shared = None;
    for (layer, raw_type) in layers.iter().enumerate() {
        let surface = terrain.surfaces().get(usize::from(*raw_type)).ok_or(
            TerrainMaterialArrayError::UnknownType {
                raw_type: *raw_type,
                surfaces: terrain.surfaces().len(),
            },
        )?;
        let appearance =
            surface
                .appearance()
                .layered
                .ok_or(TerrainMaterialArrayError::NoChannel {
                    raw_type: *raw_type,
                    channel,
                })?;
        let handle = channel.handle(appearance);
        let texture = usize::try_from(handle.raw())
            .ok()
            .and_then(|index| sources.textures.get(index))
            .ok_or(FrameSceneTextureError::Unknown {
                handle: handle.raw(),
                resident: sources.textures.len(),
            })?;
        let copyable = texture.dimension() == wgpu::TextureDimension::D2
            && texture.depth_or_array_layers() == 1
            && texture.sample_count() == 1
            && texture.usage().contains(wgpu::TextureUsages::COPY_SRC);
        if !copyable {
            return Err(TerrainMaterialArrayError::NotCopyable {
                texture: handle.raw(),
            });
        }
        let found = LayerShape::of(texture);
        let expected = *shared.get_or_insert(found);
        if found != expected {
            return Err(TerrainMaterialArrayError::Disagree {
                layer,
                expected,
                found,
            });
        }
        textures.push(texture);
    }
    let shape = shared.ok_or(TerrainMaterialArrayError::NoLayers)?;
    Ok((textures, shape))
}

/// The refusal of a request on constants that were made for no factory, which no game is lent.
fn unbound() -> TerrainMaterialArrayError {
    TerrainMaterialArrayError::Budget(GameResourceError::Declaration {
        label: ARRAY_LABEL.to_owned(),
        reason: "the engine constants are not made for a factory, so no budget is charged"
            .to_owned(),
    })
}

impl EngineConstants {
    /// Layer `i` of the returned 2D array is the resident scene texture that terrain surface
    /// type `layers[i]` names for `channel`, with all its mips, in the format the scene prepared
    /// it in (a block-compressed format stays compressed). `layers` are the raw surface types of
    /// the scene's terrain, the values `SceneTerrain::surface_indices` holds, and may repeat.
    /// The array is for the binding sourced from `semantic`, bound as a 2D array even for one
    /// layer, and is `TEXTURE_BINDING` only to a game.
    ///
    /// The layers are copied from the resident textures on the GPU, so the scene's texels are
    /// never uploaded a second time. The copies are staged on `context` and encoded into the
    /// frame's own encoder before the frame's passes draw, in the first frame that asks for the
    /// array. The array is kept for the generation once that frame has submitted; a frame that
    /// is cancelled or fails copies nothing, and the next frame that asks stages the copies
    /// again. The array is shared by every feature, and dropped when the scene's texture table
    /// or terrain changes (an append, a texture replacement or another scene), so a layer never
    /// shows a texture the scene no longer holds. Each distinct request is a resident array
    /// counted under the texture class of the census.
    ///
    /// The array's physical bytes, every mip and whole blocks for a compressed format, are also
    /// charged to the budget and the runtime ledger of the factory whose feature asks, which its
    /// declared resources count against too, and are returned when the array is dropped. A
    /// factory that asks for an array another factory made is charged for it in its own ledger,
    /// as it is for a shared texture. The charge is made before the array is created, so a
    /// refused request allocates nothing. Do not hold the factory's
    /// [`GameResources::runtime`] across the call.
    ///
    /// # Errors
    /// [`TerrainMaterialArrayError`]: `Input` when the frame in flight published no scene,
    /// `NoTerrain`, `NoLayers`, `TooManyLayers` beyond the device's `max_texture_array_layers`,
    /// `Budget` when the array's bytes with what the factory's runtime holds pass its budget,
    /// `UnknownType` and `NoChannel` for a surface type the terrain lacks or that has no layered
    /// maps, `NotCopyable` for a texture that is not a single-layer 2D texture, and `Disagree`
    /// when layers differ in format, extent or mip count.
    pub fn material_array(
        &self,
        context: &mut PrepareContext<'_>,
        semantic: EngineSemantic,
        channel: TerrainMaterialChannel,
        layers: &[u8],
    ) -> Result<Import, TerrainMaterialArrayError> {
        let charge = self.charge.as_ref().ok_or_else(unbound)?;
        let sources = self.scene.current()?;
        let terrain = sources
            .terrain
            .as_deref()
            .ok_or(TerrainMaterialArrayError::NoTerrain)?;
        let mut state = self.state.borrow_mut();
        let arrays = &mut state.terrain.arrays;
        let at = if let Some(at) = arrays
            .iter()
            .position(|array| array.channel == channel && array.layers == layers)
        {
            at
        } else {
            let array = self.build_array(&sources, terrain, (channel, layers), charge)?;
            arrays.push(array);
            arrays.len() - 1
        };
        let array = &mut arrays[at];
        array.charge_to(charge)?;
        let frame = context.publications();
        if array.copies.is_needed_in(&frame) {
            let (textures, _) = resolve(&sources, terrain, (channel, layers))?;
            stage_copies(context, &textures, &array.texture);
            array.copies.stage_in(frame);
        }
        Ok(Import::texture_array(
            semantic,
            (*array.texture).clone(),
            array.view.clone(),
        ))
    }

    /// The array for a request, made and checked but not yet filled.
    fn build_array(
        &self,
        sources: &SceneSources,
        terrain: &SceneTerrain,
        (channel, layers): (TerrainMaterialChannel, &[u8]),
        charge: &GameResources,
    ) -> Result<MaterialArray, TerrainMaterialArrayError> {
        let limit = self.device.limits().max_texture_array_layers;
        if layers.is_empty() {
            return Err(TerrainMaterialArrayError::NoLayers);
        }
        if layers.len() > limit as usize {
            return Err(TerrainMaterialArrayError::TooManyLayers {
                requested: layers.len(),
                limit,
            });
        }
        let (_, shape) = resolve(sources, terrain, (channel, layers))?;
        let count = u32::try_from(layers.len()).expect("the count is within the device limit");
        let descriptor = wgpu::TextureDescriptor {
            label: Some(ARRAY_LABEL),
            size: wgpu::Extent3d {
                width: shape.extent[0],
                height: shape.extent[1],
                depth_or_array_layers: count,
            },
            mip_level_count: shape.mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: shape.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        // Counted before the texture exists, so a refused array allocates nothing.
        let bytes = texture_backing_bytes(&descriptor);
        let hold = charge.admit(ARRAY_LABEL, bytes)?;
        let texture = self.authority.census().create_texture(
            &self.device,
            ResourceClass::Texture,
            &descriptor,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..wgpu::TextureViewDescriptor::default()
        });
        Ok(MaterialArray {
            channel,
            layers: layers.to_vec(),
            texture,
            view,
            bytes,
            holds: vec![(charge.id(), hold)],
            copies: Fill::default(),
        })
    }
}

/// Stages the copy of every mip of every source into its layer of `array` on the frame's
/// context, so it is encoded in the frame's own encoder before any pass reads the array. The
/// sources are resident scene textures that no frame writes.
fn stage_copies(
    context: &mut PrepareContext<'_>,
    sources: &[&wgpu::Texture],
    array: &wgpu::Texture,
) {
    for (layer, source) in (0..).zip(sources) {
        for mip_level in 0..array.mip_level_count() {
            // Whole blocks: a compressed mip smaller than a block copies the block it lies in.
            let size = array
                .size()
                .mip_level_size(mip_level, wgpu::TextureDimension::D2)
                .physical_size(array.format());
            context.copy_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: source,
                    mip_level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: array,
                    mip_level,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    depth_or_array_layers: 1,
                    ..size
                },
            );
        }
    }
}
