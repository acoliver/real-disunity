//! Retained HDR effects. GPU state belongs to admitted frame slots, not queue writes.
use crate::{FrameTargets, PostError, Stage, StageSet};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveExposure {
    pub log_range: [f32; 2],
    pub percentiles: [f32; 2],
    pub key: f32,
    pub exposure_range: [f32; 2],
    pub speeds: [f32; 2],
    pub fixed_delta: Option<f32>,
    pub fixed_exposure: Option<f32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bloom {
    pub threshold: f32,
    pub knee: f32,
    pub intensity: f32,
    pub levels: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HdrPostSettings {
    pub adaptive: Option<AdaptiveExposure>,
    pub bloom: Option<Bloom>,
    pub fxaa: Option<f32>,
    pub upscale: Option<crate::UpscaleSettings>,
}
impl HdrPostSettings {
    /// Validate the HDR display path before allocating frame targets.
    pub fn validate_frame(self, config: crate::FrameConfig) -> Result<(), PostError> {
        self.validate()?;
        if config.size != config.output_extent() && self.upscale.is_none() {
            return Err(PostError::UnsupportedProfile(
                "reduced HDR extent requires an upscale mode",
            ));
        }
        Ok(())
    }

    pub fn validate(self) -> Result<(), PostError> {
        if let Some(upscale) = self.upscale {
            upscale.validate()?;
        }
        if let Some(a) = self.adaptive {
            let values = [
                a.log_range[0],
                a.log_range[1],
                a.percentiles[0],
                a.percentiles[1],
                a.key,
                a.exposure_range[0],
                a.exposure_range[1],
                a.speeds[0],
                a.speeds[1],
            ];
            if values.iter().any(|v| !v.is_finite())
                || a.log_range[0] >= a.log_range[1]
                || a.log_range[0] < -24.0
                || a.log_range[1] > 24.0
                || a.percentiles[0] < 0.0
                || a.percentiles[1] > 1.0
                || a.percentiles[0] >= a.percentiles[1]
                || a.key <= 0.0
                || a.key > 1.0
                || a.exposure_range[0] <= 0.0
                || a.exposure_range[0] > a.exposure_range[1]
                || a.exposure_range[1] > 64.0
                || a.speeds.iter().any(|v| *v <= 0.0 || *v > 100.0)
                || a.fixed_delta
                    .is_some_and(|v| !v.is_finite() || !(0.0..=10.0).contains(&v))
                || a.fixed_exposure
                    .is_some_and(|v| !v.is_finite() || !(0.0..=64.0).contains(&v))
            {
                return Err(PostError::UnsupportedProfile("adaptive exposure settings"));
            }
        }
        if self.bloom.is_some_and(|b| {
            !b.threshold.is_finite()
                || !(0.0..=64.0).contains(&b.threshold)
                || !b.knee.is_finite()
                || !(0.0..=1.0).contains(&b.knee)
                || !b.intensity.is_finite()
                || !(0.0..=2.0).contains(&b.intensity)
                || !(1..=8).contains(&b.levels)
        }) {
            return Err(PostError::UnsupportedProfile("bloom settings"));
        }
        if self
            .fxaa
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(PostError::UnsupportedProfile("FXAA settings"));
        }
        Ok(())
    }
    pub fn enabled(self) -> bool {
        self.adaptive.is_some()
            || self.bloom.is_some()
            || self.fxaa.is_some()
            || self.upscale.is_some()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HdrPost {
    pub settings: HdrPostSettings,
    device: wgpu::Device,
    pub exposure: [wgpu::Buffer; 3],
    carry: wgpu::Buffer,
    frame: [wgpu::Buffer; 3],
}
impl HdrPost {
    pub fn new(device: &wgpu::Device, settings: HdrPostSettings) -> Result<Self, PostError> {
        settings.validate()?;
        Ok(Self {
            settings,
            device: device.clone(),
            carry: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("queue-ordered exposure carry"),
                size: 16,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            exposure: std::array::from_fn(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("slot exposure result"),
                    size: 16,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
            frame: std::array::from_fn(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("slot exposure step"),
                    size: 16,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            }),
        })
    }
    pub fn prepare(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        delta: f32,
        reset: bool,
        initial: f32,
    ) -> Result<(), PostError> {
        let delta = self
            .settings
            .adaptive
            .and_then(|a| a.fixed_delta)
            .unwrap_or(delta);
        validate_delta(delta)?;
        let bytes: Vec<_> = [delta.to_bits(), u32::from(reset), initial.to_bits(), 0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let staging = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("immutable exposure step"),
                contents: &bytes,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
        encoder.copy_buffer_to_buffer(&staging, 0, &self.frame[slot], 0, 16);
        Ok(())
    }
    pub fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        slot: usize,
        format: wgpu::TextureFormat,
        initial: f64,
    ) -> StageSet {
        let mut set = StageSet::default();
        let exposure = &self.exposure[slot];
        if let Some(a) = self.settings.adaptive {
            self.meter(device, targets, slot, a, &mut set);
            set.stages.push(Stage {
                id: crate::StageId::new("publish frame exposure", 0),
                label: "publish frame exposure",
                enabled: true,
                operation: crate::Operation::CopyBuffer {
                    source: self.carry.clone(),
                    destination: self.exposure[slot].clone(),
                    bytes: 16,
                },
            });
        }
        crate::post_filters::Filters::new(device, &mut set).build(
            targets,
            self.settings,
            exposure,
            (format, initial),
        );
        set
    }

    fn meter(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        slot: usize,
        a: AdaptiveExposure,
        set: &mut StageSet,
    ) {
        let bins = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("slot luminance histogram"),
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("luminance histogram and adaptation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("exposure.wgsl").into()),
        });
        let constants = exposure_constants(a);
        set.stages.push(Stage {
            id: crate::StageId::new("clear luminance histogram", 0),
            label: "clear luminance histogram",
            enabled: true,
            operation: crate::Operation::Clear(bins.clone()),
        });
        for (entry, dispatch) in [
            (
                "histogram",
                [
                    targets.config().size[0].div_ceil(8),
                    targets.config().size[1].div_ceil(8),
                    1,
                ],
            ),
            ("reduce", [1, 1, 1]),
        ] {
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                cache: None,
            });
            let entries = self.meter_entries(targets, slot, &bins, entry);
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(entry),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &entries,
            });
            set.stages.push(Stage {
                id: crate::StageId::new(entry, 0),
                label: entry,
                enabled: true,
                operation: crate::Operation::Compute {
                    pipeline,
                    groups: vec![group],
                    dispatch,
                },
            });
        }
        set.resources.buffers.push(bins);
    }
    fn meter_entries<'a>(
        &'a self,
        targets: &'a FrameTargets,
        slot: usize,
        bins: &'a wgpu::Buffer,
        entry: &str,
    ) -> Vec<wgpu::BindGroupEntry<'a>> {
        if entry == "histogram" {
            vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(targets.resolved()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bins.as_entire_binding(),
                },
            ]
        } else {
            vec![
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bins.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.carry.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.frame[slot].as_entire_binding(),
                },
            ]
        }
    }
}

/// CPU reference: 256 equal log2 bins, clamped endpoints, finite nonnegative input.
pub fn histogram_bin(luminance: f32, range: [f32; 2]) -> usize {
    let value = if luminance.is_finite() {
        luminance.max(0.0)
    } else {
        0.0
    };
    let t =
        ((value.max(range[0].exp2()).log2() - range[0]) / (range[1] - range[0])).clamp(0.0, 1.0);
    (0..255)
        .find(|&bin| t < f32::from(u16::try_from(bin + 1).unwrap()) / 256.0)
        .unwrap_or(255)
}
pub fn target_exposure(bins: &[u32; 256], a: AdaptiveExposure) -> f64 {
    let total: f64 = bins.iter().map(|&v| f64::from(v)).sum();
    let low = total * f64::from(a.percentiles[0]);
    let high = total * f64::from(a.percentiles[1]);
    let mut prefix: f64 = 0.0;
    let mut sum: f64 = 0.0;
    let mut retained: f64 = 0.0;
    for (i, &count) in bins.iter().enumerate() {
        let weight = ((prefix + f64::from(count)).min(high) - prefix.max(low)).max(0.0);
        let center = f64::from(a.log_range[0])
            + (f64::from(u16::try_from(i).unwrap()) + 0.5)
                * f64::from(a.log_range[1] - a.log_range[0])
                / 256.0;
        sum += weight * center;
        retained += weight;
        prefix += f64::from(count);
    }
    let target = f64::from(a.key) / (sum / retained.max(0.000_001)).exp2();
    // The result is bounded to the validated finite f32 exposure interval.
    target.clamp(
        f64::from(a.exposure_range[0]),
        f64::from(a.exposure_range[1]),
    )
}
pub fn adapt_exposure(previous: f32, target: f32, delta: f32, speeds: [f32; 2]) -> f32 {
    let speed = if target > previous {
        speeds[0]
    } else {
        speeds[1]
    };
    previous + (target - previous) * (1.0 - (-speed * delta).exp())
}

fn exposure_constants(a: AdaptiveExposure) -> [(&'static str, f64); 10] {
    [
        ("LOG_MIN", f64::from(a.log_range[0])),
        ("LOG_MAX", a.log_range[1].into()),
        ("LOW", a.percentiles[0].into()),
        ("HIGH", a.percentiles[1].into()),
        ("KEY", a.key.into()),
        ("MIN_EXPOSURE", a.exposure_range[0].into()),
        ("MAX_EXPOSURE", a.exposure_range[1].into()),
        ("SPEED_UP", a.speeds[0].into()),
        ("SPEED_DOWN", a.speeds[1].into()),
        ("FIXED_EXPOSURE", a.fixed_exposure.unwrap_or(-1.0).into()),
    ]
}

pub(crate) fn validate_delta(delta: f32) -> Result<(), PostError> {
    if !delta.is_finite() || !(0.0..=10.0).contains(&delta) {
        return Err(PostError::FrameDelta);
    }
    Ok(())
}
