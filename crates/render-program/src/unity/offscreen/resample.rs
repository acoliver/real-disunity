//! A blit with no material between textures that differ in size or format.
//!
//! The record says only "a full-surface draw of the source into the target; without a material
//! the draw copies". A copy between other sizes or formats is therefore the source drawn over
//! the whole target, sampled with its own filter and wrap, by a built-in program that has no
//! mesh: one triangle that covers the target, whose texture coordinates run over 0 to 1 across
//! the target (scale one, offset zero). Sampling decodes sRGB and block formats to linear
//! values and the target encodes them, so sRGB to sRGB keeps its values. Only the first level
//! of the first layer is read, the level a copy of the same size reads, and the first level of
//! the first layer of the target is written.

use std::sync::Arc;

use worldrunner_render_post::AttachmentLoad;
use worldrunner_render_post::contracts::{
    ColorAttachment, ResourceUse, SamplerBinding, Semantic, TextureBinding, TextureBindingKind,
    UseKind,
};

use crate::binding::{
    BindingAddress, BindingSource, PropertyId, ResourceBinding, ResourceKind, ResourceLayout,
};
use crate::error::ProgramError;
use crate::ids::TextureId;
use crate::module::{StageDeclarations, StageKind, StageModule, StageProgram, StageSource};
use crate::program::GraphicsProgram;

use super::error::OffscreenError;
use super::plan::{Services, read_unfilterable};
use super::steps::{Step, raster_id};

const SHADER: &str = "
struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@vertex fn vs(@builtin(vertex_index) corner: u32) -> Varying {
    let x = f32((corner << 1u) & 2u);
    let y = f32(corner & 2u);
    var out: Varying;
    out.position = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}
@fragment fn fs(in: Varying) -> @location(0) vec4<f32> {
    return textureSampleLevel(source, source_sampler, in.uv, 0.0);
}";

/// A resample of the first level of `source` over the first level of the target, whose bind
/// group waits for the finish like the other steps' do.
pub(super) struct ResampleStep {
    pub(super) id: worldrunner_render_post::contracts::RasterId,
    pub(super) color: ColorAttachment,
    pub(super) program: GraphicsProgram,
    pub(super) texture: TextureBinding,
    pub(super) sampler: SamplerBinding,
    pub(super) uses: Vec<ResourceUse>,
}

fn declarations(stage: StageKind) -> StageDeclarations {
    if stage == StageKind::Vertex {
        return StageDeclarations::default();
    }
    let visibility = wgpu::ShaderStages::FRAGMENT;
    StageDeclarations {
        resources: ResourceLayout::new(vec![
            ResourceBinding {
                at: BindingAddress { set: 0, binding: 0 },
                source: BindingSource::Property(PropertyId::new("source")),
                visibility,
                kind: ResourceKind::Texture {
                    dimension: wgpu::TextureViewDimension::D2,
                    sample: wgpu::TextureSampleType::Float { filterable: true },
                    multisampled: false,
                },
            },
            ResourceBinding {
                at: BindingAddress { set: 0, binding: 1 },
                source: BindingSource::Unnamed,
                visibility,
                kind: ResourceKind::Sampler(wgpu::SamplerBindingType::Filtering),
            },
        ]),
        constants: Vec::new(),
    }
}

fn stage(entry: &str, kind: StageKind) -> Result<StageProgram, ProgramError> {
    let module = StageModule {
        source: StageSource::Wgsl(Arc::from(SHADER)),
        entry_point: entry.to_owned(),
        kind,
    };
    Ok(StageProgram::new(module, declarations(kind))?)
}

/// The built-in program, made when the first resample needs it.
fn program(svc: &mut Services<'_>) -> Result<GraphicsProgram, OffscreenError> {
    if let Some(program) = svc.resample.as_ref() {
        return Ok(program.clone());
    }
    let program = svc.runtime.graphics_program(
        &stage("vs", StageKind::Vertex)?,
        &stage("fs", StageKind::Fragment)?,
    )?;
    *svc.resample = Some(program.clone());
    Ok(program)
}

/// Plans the draw of `source` over `target`.
pub(super) fn resample_step(
    svc: &mut Services<'_>,
    index: usize,
    (source, target): (TextureId, TextureId),
) -> Result<Step, OffscreenError> {
    let needs_material = |reason: String| OffscreenError::BlitNeedsMaterial { reason };
    let info = svc.textures.get(source)?;
    if source == target {
        return Err(needs_material(
            "the source and the target are one texture".to_owned(),
        ));
    }
    if info.color.is_none() {
        return Err(needs_material(format!(
            "the source format {:?} holds depth, which a plain blit does not sample",
            info.format
        )));
    }
    let features = svc.runtime.device().features();
    let declared = wgpu::TextureSampleType::Float { filterable: true };
    let (sample_type, state) = read_unfilterable(declared, info.format, features, info.sampler);
    let usable = info
        .format
        .sample_type(Some(wgpu::TextureAspect::All), Some(features));
    if !matches!(usable, Some(wgpu::TextureSampleType::Float { .. })) {
        return Err(needs_material(format!(
            "the source format {:?} is not sampled as floating-point colors",
            info.format
        )));
    }
    let program = program(svc)?;
    let (imported, _) = svc.import(source)?;
    let version = svc.tracker.cell(imported.primary, 0, 0);
    let view = svc.tracker.level_view(
        imported.primary,
        (0, 0),
        wgpu::TextureViewDimension::D2,
        version,
    )?;
    let (target_imported, _) = svc.import(target)?;
    let color = target_imported
        .color
        .ok_or(OffscreenError::NoColorAttachment(target))?;
    let (before, after) = svc.tracker.write(color, 0, 0);
    let target_view = svc
        .tracker
        .cell_view(color, (0, 0), wgpu::TextureAspect::All, after)?;
    let visibility = wgpu::ShaderStages::FRAGMENT;
    Ok(Step::Resample(Box::new(ResampleStep {
        id: raster_id("offscreen blit", index),
        color: ColorAttachment {
            view: target_view,
            before: Some(before),
            load: AttachmentLoad::Load,
            store: wgpu::StoreOp::Store,
            resolve: None,
        },
        program,
        uses: vec![ResourceUse::read(view.clone(), UseKind::Sampled)],
        texture: TextureBinding {
            binding: 0,
            visibility,
            view,
            samples: 1,
            semantic: Semantic::Unspecified,
            kind: TextureBindingKind::Sampled(sample_type),
        },
        sampler: SamplerBinding {
            binding: 1,
            visibility,
            sampler: svc.samplers.get(state),
        },
    })))
}
