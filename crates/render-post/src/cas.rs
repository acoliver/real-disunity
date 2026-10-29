//! Contrast adaptive sharpening settings. The shader in `cas.wgsl` ports the sharpen-only
//! path of AMD `FidelityFX` CAS (<https://github.com/GPUOpen-Effects/FidelityFX-CAS>, MIT).
use crate::{PostError, Stage, StageId, StageSet, StageTarget};

/// Stage identity, stable for timing.
pub(crate) const STAGE: &str = "cas";

/// Contrast adaptive sharpening of the display-referred image after the antialiasing resolve.
///
/// Sharpening is off when the choice is `None`. Every `Some` schedules one pass, including
/// sharpness 0: that is the reference's low-ringing default and still sharpens.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CasSettings {
    /// The reference's sharpness knob, finite and within 0..=1. 0 is the reference default
    /// with the least ringing and 1 is the strongest sharpening.
    pub sharpness: f32,
}
impl CasSettings {
    pub const fn new(sharpness: f32) -> Self {
        Self { sharpness }
    }

    pub fn validate(self) -> Result<(), PostError> {
        if self.sharpness.is_finite() && (0.0..=1.0).contains(&self.sharpness) {
            Ok(())
        } else {
            Err(PostError::UnsupportedProfile("CAS settings"))
        }
    }

    /// Negative lobe weight of the filter ring at full local contrast, from the reference
    /// `CasSetup`: `-1 / lerp(8, 5, sharpness)`, which runs from -1/8 to -1/5.
    pub(crate) fn peak(self) -> f32 {
        -(8.0 - 3.0 * self.sharpness).recip()
    }
}

/// Appends the one sharpening pass. `source` is display-referred color in a format that
/// `textureLoad` reads, at the extent of `output`; alpha is copied unchanged.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    source: &wgpu::TextureView,
    settings: CasSettings,
    output: (wgpu::TextureFormat, StageTarget),
) {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("CAS sharpen-only"),
        source: wgpu::ShaderSource::Wgsl(include_str!("cas.wgsl").into()),
    });
    let layout = crate::upscale::source_layout(device, STAGE);
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(STAGE),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = crate::fullscreen_pipeline(
        device,
        crate::FullscreenDescriptor {
            label: Some(STAGE),
            layout: &pipeline_layout,
            module: &module,
            vertex_entry: "vertex",
            fragment_entry: STAGE,
            format: output.0,
            constants: &[("PEAK", settings.peak().into())],
        },
    );
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(STAGE),
        layout: &layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(source),
        }],
    });
    set.stages.push(Stage::fullscreen(
        StageId::new(STAGE, 0),
        &pipeline,
        &[&group],
        output.1,
    ));
}
