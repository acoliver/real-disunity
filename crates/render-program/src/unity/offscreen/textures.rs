//! The textures offscreen work names: uploaded sources and render textures the state creates.

use std::collections::{BTreeSet, HashMap};

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
    pub(super) sampler: Option<SamplerState>,
    /// The spec a render texture was made from; `None` for an uploaded source.
    pub(super) spec: Option<RenderTextureSpec>,
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
            sampler: source.sampler,
            spec: None,
        };
        self.replace(runtime, source.id, info);
        Ok(())
    }

    /// Creates every render texture of the frame that is new or whose spec changed.
    pub(super) fn sync(
        &mut self,
        runtime: &mut ProgramRuntime,
        specs: &[RenderTextureSpec],
    ) -> Result<(), OffscreenError> {
        self.release_waiting(runtime);
        for spec in specs {
            let current = self.textures.get(&spec.id).and_then(|t| t.spec.as_ref());
            if current != Some(spec) {
                let info = create_render_texture(runtime, spec)?;
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

fn create_render_texture(
    runtime: &mut ProgramRuntime,
    spec: &RenderTextureSpec,
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
                    ..shape(format)
                }
            } else {
                shape(format)
            };
            runtime.create_empty_texture("offscreen render texture depth", shape)
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
        sampler: None,
        spec: Some(spec.clone()),
    })
}
