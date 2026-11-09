//! The textures offscreen work names: uploaded sources and render textures the state creates.

use std::collections::{BTreeSet, HashMap};

use worldrunner_render_gpu::configuration::AllocationSet;

use crate::create::TextureShape;
use crate::error::ProgramError;
use crate::ids::TextureId;
use crate::resources::TextureHandle;
use crate::runtime::ProgramRuntime;
use crate::sampler::SamplerState;

use super::error::OffscreenError;
use super::format::{depth_format, render_color_format, source_format};
use super::frame::{RenderTextureSpec, SourceTexture};

/// A texture of the state and what its color and depth storage is.
pub(super) struct TextureInfo {
    pub(super) color: Option<TextureHandle>,
    pub(super) depth: Option<TextureHandle>,
    pub(super) size: [u32; 2],
    pub(super) layers: u32,
    pub(super) mips: u32,
    /// The color format, or the depth format of a depth-only render texture.
    pub(super) format: wgpu::TextureFormat,
    pub(super) depth_format: Option<wgpu::TextureFormat>,
    /// Whether the color storage allows storage binding, for a kernel to write.
    pub(super) storage: bool,
    pub(super) sampler: Option<SamplerState>,
    /// The spec a render texture was made from; `None` for an uploaded source.
    pub(super) spec: Option<RenderTextureSpec>,
    /// For an uploaded source, its texels (blocks, for a block-compressed format) and how many of
    /// them hold a byte other than zero.
    pub(super) counts: Option<(usize, usize)>,
    /// The bytes it holds in the budget: the source's bytes, or the render texture's color levels
    /// and depth buffer.
    pub(super) held_bytes: u64,
}

impl TextureInfo {
    pub(super) fn handles(&self) -> impl Iterator<Item = TextureHandle> {
        self.color.into_iter().chain(self.depth)
    }

    /// The storage copies, readbacks and samples use: the color, or the depth when there is none.
    pub(super) fn primary(&self) -> TextureHandle {
        self.color
            .or(self.depth)
            .expect("a texture has color or depth storage")
    }
}

/// The textures by id. A render texture is created when the frame first names it and again when
/// its spec changes; the replaced storage is released as soon as the runtime is not recording.
#[derive(Default)]
pub(super) struct TextureTable {
    textures: HashMap<TextureId, TextureInfo>,
    retired: Vec<TextureHandle>,
    /// The render textures a kernel binds as storage images. A texture stays in the set, so it is
    /// made again for storage only once.
    storage: BTreeSet<TextureId>,
}

impl TextureTable {
    pub(super) fn get(&self, id: TextureId) -> Result<&TextureInfo, OffscreenError> {
        self.textures
            .get(&id)
            .ok_or(OffscreenError::UnknownTexture(id))
    }

    /// Uploads a source texture, replacing the one of that id.
    pub(super) fn set_source(
        &mut self,
        runtime: &mut ProgramRuntime,
        source: &SourceTexture,
    ) -> Result<(), OffscreenError> {
        let format = source_format(source.graphics_format)?;
        let color =
            runtime.create_texture_2d("offscreen source", source.size, format, &source.bytes)?;
        let info = TextureInfo {
            color: Some(color),
            depth: None,
            size: source.size,
            layers: 1,
            mips: 1,
            format,
            depth_format: None,
            storage: false,
            sampler: source.sampler,
            spec: None,
            counts: Some(nonzero_texels(&source.bytes, format)),
            held_bytes: source.bytes.len() as u64,
        };
        self.replace(runtime, source.id, info);
        Ok(())
    }

    /// Marks render textures as ones a kernel binds as storage images.
    pub(super) fn bind_as_storage(&mut self, ids: impl IntoIterator<Item = TextureId>) {
        self.storage.extend(ids);
    }

    /// Creates every render texture of the frame that is new, whose spec changed or that a kernel
    /// binds as storage and was made without storage binding.
    pub(super) fn sync(
        &mut self,
        runtime: &mut ProgramRuntime,
        specs: &[RenderTextureSpec],
    ) -> Result<(), OffscreenError> {
        self.release_waiting(runtime);
        for spec in specs {
            let storage = self.storage.contains(&spec.id) && storage_format(runtime, spec);
            let held = self.textures.get(&spec.id);
            let current = held.and_then(|t| t.spec.as_ref());
            if current != Some(spec) || held.is_some_and(|t| t.storage != storage) {
                let info = create_render_texture(runtime, spec, storage)?;
                self.replace(runtime, spec.id, info);
            }
        }
        Ok(())
    }

    /// Gives back the storage of every render texture that `live` does not name, as one that its
    /// owner destroyed. An uploaded source is kept: it is replaced, never destroyed.
    pub(super) fn retain_render_textures(
        &mut self,
        runtime: &mut ProgramRuntime,
        live: &BTreeSet<TextureId>,
    ) {
        let gone: Vec<TextureId> = self
            .textures
            .iter()
            .filter(|(id, info)| info.spec.is_some() && !live.contains(id))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(old) = self.textures.remove(&id) {
                self.retired.extend(old.handles());
            }
        }
        self.release_waiting(runtime);
    }

    /// Gives back the storage of the uploaded source `id` and returns its bytes. A render texture
    /// is not released here, and nothing is for an id that names none.
    pub(super) fn release_source(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: TextureId,
    ) -> Option<u64> {
        if self.textures.get(&id)?.spec.is_some() {
            return None;
        }
        let old = self.textures.remove(&id)?;
        self.retired.extend(old.handles());
        self.release_waiting(runtime);
        Some(old.held_bytes)
    }

    /// The render textures the table holds and the bytes of each, in id order.
    pub(super) fn render_held(&self) -> Vec<(TextureId, u64)> {
        let mut held: Vec<_> = self
            .textures
            .iter()
            .filter(|(_, info)| info.spec.is_some())
            .map(|(id, info)| (*id, info.held_bytes))
            .collect();
        held.sort_unstable();
        held
    }

    /// Gives back the storage of the render texture `id` and returns its bytes. An uploaded source
    /// and an id that names nothing are left alone.
    pub(super) fn release_render(
        &mut self,
        runtime: &mut ProgramRuntime,
        id: TextureId,
    ) -> Option<u64> {
        self.textures.get(&id)?.spec.as_ref()?;
        let old = self.textures.remove(&id)?;
        self.retired.extend(old.handles());
        self.release_waiting(runtime);
        Some(old.held_bytes)
    }

    /// The bytes of staging a copy of the first mip of `id` into a buffer takes.
    pub(super) fn read_bytes(&self, id: TextureId) -> Option<u64> {
        let info = self.textures.get(&id)?;
        let texel = u64::from(info.format.block_copy_size(None)?);
        let row = (u64::from(info.size[0]) * texel).next_multiple_of(256);
        Some(row * u64::from(info.size[1]))
    }

    /// The bytes the render textures of `specs` would add: those the table has not made or made
    /// from another spec. The storage a replaced texture held is given back later and is not
    /// subtracted.
    pub(super) fn new_render_bytes(
        &self,
        runtime: &ProgramRuntime,
        specs: &[RenderTextureSpec],
    ) -> u64 {
        let limits = runtime.device().limits();
        specs
            .iter()
            .filter(|spec| self.textures.get(&spec.id).and_then(|t| t.spec.as_ref()) != Some(*spec))
            .map(|spec| render_bytes(spec, &limits))
            .sum()
    }

    fn replace(&mut self, runtime: &mut ProgramRuntime, id: TextureId, info: TextureInfo) {
        if let Some(old) = self.textures.insert(id, info) {
            self.retired.extend(old.handles());
            self.release_waiting(runtime);
        }
    }

    /// Releases replaced storage. A release the runtime refuses because a frame is being
    /// recorded is tried again by the next call.
    fn release_waiting(&mut self, runtime: &mut ProgramRuntime) {
        self.retired
            .retain(|handle| matches!(runtime.release_texture(*handle), Err(ProgramError::Busy)));
    }
}

/// Whether the device binds the color format of `spec` as a storage image, so that the texture
/// can be made for a kernel to write.
fn storage_format(runtime: &ProgramRuntime, spec: &RenderTextureSpec) -> bool {
    render_color_format(spec.graphics_format)
        .ok()
        .flatten()
        .is_some_and(|format| {
            runtime.supports_storage_image(format, wgpu::StorageTextureAccess::WriteOnly)
        })
}

fn create_render_texture(
    runtime: &mut ProgramRuntime,
    spec: &RenderTextureSpec,
    storage: bool,
) -> Result<TextureInfo, OffscreenError> {
    if spec.size.contains(&0) || spec.layers == 0 || spec.mips == 0 {
        return Err(OffscreenError::EmptyTexture { id: spec.id });
    }
    let color_format = render_color_format(spec.graphics_format)?;
    let depth_format = depth_format(spec.depth_bits)?;
    let Some(format) = color_format.or(depth_format) else {
        return Err(OffscreenError::UnsupportedFormat {
            code: spec.graphics_format,
        });
    };
    if format.is_srgb() != spec.srgb {
        return Err(OffscreenError::SrgbMismatch {
            id: spec.id,
            srgb: spec.srgb,
            linear: format.is_srgb(),
        });
    }
    let shape = |format| TextureShape {
        size: spec.size,
        layers: spec.layers,
        mips: spec.mips,
        format,
        storage,
    };
    let color = color_format
        .map(|format| runtime.create_empty_texture("offscreen render texture", shape(format)))
        .transpose()?;
    // A depth buffer under a color texture is one layer and one mip, which the camera draws into.
    // Without color the depth is the texture and has the spec's layers and mips.
    let depth = depth_format
        .map(|format| {
            let shape = if color.is_some() {
                TextureShape {
                    layers: 1,
                    mips: 1,
                    storage: false,
                    ..shape(format)
                }
            } else {
                shape(format)
            };
            runtime.create_empty_texture(
                "offscreen render texture depth",
                TextureShape {
                    storage: false,
                    ..shape
                },
            )
        })
        .transpose();
    let depth = match depth {
        Ok(depth) => depth,
        Err(error) => {
            // The color storage is not used by anything yet, so it is released at once.
            if let Some(color) = color {
                let _ = runtime.release_texture(color);
            }
            return Err(error.into());
        }
    };
    Ok(TextureInfo {
        color,
        depth,
        size: spec.size,
        layers: spec.layers,
        mips: spec.mips,
        format,
        depth_format,
        storage: color.is_some() && storage,
        sampler: None,
        spec: Some(spec.clone()),
        counts: None,
        held_bytes: render_bytes(spec, &runtime.device().limits()),
    })
}

/// The bytes a render texture of `spec` takes: its color levels and its depth buffer. A spec the
/// state cannot make takes none.
fn render_bytes(spec: &RenderTextureSpec, limits: &wgpu::Limits) -> u64 {
    let (Ok(color), Ok(depth)) = (
        render_color_format(spec.graphics_format),
        depth_format(spec.depth_bits),
    ) else {
        return 0;
    };
    let mut allocations = AllocationSet::default();
    if let Some(format) = color {
        allocations.texture(
            "render texture",
            [spec.size[0], spec.size[1], spec.layers],
            format,
            [spec.mips, 1],
            1,
        );
    }
    if let Some(format) = depth {
        // Under a color texture the depth is one layer and one level, else it is the texture.
        let (layers, mips) = if color.is_some() {
            (1, 1)
        } else {
            (spec.layers, spec.mips)
        };
        allocations.texture(
            "render texture depth",
            [spec.size[0], spec.size[1], layers],
            format,
            [mips, 1],
            1,
        );
    }
    allocations.validate(limits).unwrap_or(0)
}

/// How many texels `bytes` holds of `format`, and how many have a byte other than zero.
fn nonzero_texels(bytes: &[u8], format: wgpu::TextureFormat) -> (usize, usize) {
    let stride = format.block_copy_size(None).map_or(1, |size| size as usize);
    let texels = bytes.chunks_exact(stride);
    let total = texels.len();
    let nonzero = texels
        .filter(|texel| texel.iter().any(|byte| *byte != 0))
        .count();
    (total, nonzero)
}
