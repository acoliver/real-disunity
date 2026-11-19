//! Generating the mip levels of a render texture from its base level.
//!
//! The engine writes the base level of a render texture and no other, so a texture a scene
//! samples with its whole chain holds nothing in the levels below. A [`GenerateMips`] record
//! fills them: level `n` is drawn from level `n - 1` of the same layer or cube face by the
//! built-in resample program, reading through a bilinear, edge-clamped sampler at the centre of
//! each destination texel, which for a source twice the size is the 2x2 box average of the texels
//! it covers. A level whose side halves from an odd one reads the texel the centre falls between
//! and is not an exact box. sRGB formats are read as linear values and written encoded, so the
//! average is taken in linear light.
//!
//! The pass of every layer of a level takes one version of the texture's storage for that level, so
//! a later draw sees each level written at one point, as it does the base of a plain texture, and
//! binds the whole chain. The texture must be a 2D, array or cube color render texture of a format
//! the device filters as floating-point values; 3D textures, depth textures and other formats
//! are refused by name.
//!
//! [`GenerateMips`]: super::frame::Record::GenerateMips

use worldrunner_render_post::contracts::{
    AllocationKey, CheckedSampler, ColorAttachment, RasterId,
};
use worldrunner_render_post::{AttachmentLoad, StageId};

use crate::ids::TextureId;
use crate::program::GraphicsProgram;
use crate::sampler::{Filter, SamplerState, Wrap};

use super::error::OffscreenError;
use super::plan::Services;
use super::resample::{program, sampled_draw};
use super::steps::{Step, require_render_texture};

/// How a level reads the one above it.
const DOWNSAMPLE: SamplerState = SamplerState::new(Wrap::Clamp, Wrap::Clamp, Filter::Bilinear);

const FILTERABLE: wgpu::TextureSampleType = wgpu::TextureSampleType::Float { filterable: true };

/// What every pass of one texture's chain shares.
struct Chain {
    color: AllocationKey,
    program: GraphicsProgram,
    sampler: CheckedSampler,
    /// The number the first pass of the chain has among the frame's steps. The contract names
    /// each operation by its id, so each pass takes the next number.
    first: usize,
}

impl Chain {
    /// The pass that draws `layer` of `mip` from `layer` of the level above, whose cells are
    /// at the versions the planning so far left them, as the version `written`.
    fn pass(
        &self,
        svc: &Services<'_>,
        (mip, layer): (u32, u32),
        (written, ordinal): (u32, usize),
    ) -> Result<Step, OffscreenError> {
        let above = (mip - 1, layer);
        let version = svc.tracker.cell(self.color, above.0, above.1);
        let source =
            svc.tracker
                .level_view(self.color, above, wgpu::TextureViewDimension::D2, version)?;
        let view =
            svc.tracker
                .cell_view(self.color, (mip, layer), wgpu::TextureAspect::All, written)?;
        // The triangle covers the level, so nothing of what it held is read.
        let color = ColorAttachment {
            view,
            before: None,
            load: AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT),
            store: wgpu::StoreOp::Store,
            resolve: None,
        };
        Ok(sampled_draw(
            RasterId::new(
                StageId::new("offscreen mips", mip),
                u32::try_from(self.first + ordinal).unwrap_or(u32::MAX),
            ),
            self.program.clone(),
            (source, FILTERABLE, self.sampler.clone()),
            color,
        ))
    }
}

/// Plans the passes that fill the levels below the base of `texture`, from the top level down and
/// each level in layer order, the first of which is step `first` of the frame. A texture of one
/// level has none.
pub(super) fn generate_steps(
    svc: &mut Services<'_>,
    first: usize,
    texture: TextureId,
) -> Result<Vec<Step>, OffscreenError> {
    require_render_texture(svc, texture)?;
    let info = svc.textures.get(texture)?;
    if info.dimension.is_volume() {
        return Err(OffscreenError::MipsOfVolume(texture));
    }
    let (mips, layers, format) = (info.mips, info.layers, info.format);
    let features = svc.runtime.device().features();
    let sampled = format.sample_type(Some(wgpu::TextureAspect::All), Some(features));
    if info.color.is_none() || sampled != Some(FILTERABLE) {
        return Err(OffscreenError::MipsOfFormat { texture, format });
    }
    if mips < 2 {
        return Ok(Vec::new());
    }
    let program = program(svc)?;
    let (imported, _) = svc.import(texture)?;
    let color = imported
        .color
        .ok_or(OffscreenError::NoColorAttachment(texture))?;
    let chain = Chain {
        color,
        program,
        sampler: svc.samplers.get(Some(DOWNSAMPLE)),
        first,
    };
    let mut steps = Vec::new();
    for mip in 1..mips {
        let written = svc.tracker.write_level(color, mip, layers);
        for layer in 0..layers {
            steps.push(chain.pass(svc, (mip, layer), (written, steps.len()))?);
        }
    }
    Ok(steps)
}
