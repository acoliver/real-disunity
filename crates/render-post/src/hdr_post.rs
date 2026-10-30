//! Retained HDR effects. GPU state belongs to admitted frame slots, not queue writes.
use crate::{FrameTargets, PostError, Stage, StageSet};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveExposure {
    pub metering: crate::ExposureMetering,
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
    pub policies: crate::BloomPolicies,
    pub threshold: f32,
    pub knee: f32,
    pub intensity: f32,
    pub levels: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HdrPostSettings {
    pub grading: crate::ColorGrading,
    pub tone_map: Option<crate::ToneMap>,
    pub adaptive: Option<AdaptiveExposure>,
    pub bloom: Option<Bloom>,
    pub bloom_kernel: crate::BloomKernel,
    /// Post-process antialiasing applied after tone mapping.
    pub resolve: crate::PostResolve,
    /// Contrast adaptive sharpening after the resolve and before any upscale.
    pub sharpen: Option<crate::CasSettings>,
    pub upscale: Option<crate::UpscaleSettings>,
    /// Opt-in RCAS at equal render/output extents; ignored when upscaling.
    pub native_rcas: Option<crate::NativeRcasSettings>,
}
impl HdrPostSettings {
    /// Validate the HDR display path before allocating frame targets.
    ///
    /// `UpscaleMode::Temporal` has its own rows, see the Temporal rows in `upscale_temporal.rs`.
    pub fn validate_frame(self, config: crate::FrameConfig) -> Result<(), PostError> {
        self.validate_frame_inner(config)?;
        self.validate_temporal(config, None)
    }

    fn validate_frame_inner(self, config: crate::FrameConfig) -> Result<(), PostError> {
        self.validate()?;
        if config.size != config.output_extent() && self.upscale.is_none() {
            return Err(PostError::UnsupportedProfile(
                "reduced HDR extent requires an upscale mode",
            ));
        }
        // Gaussian levels run at the render extent and must not be smaller than 1x1.
        if let (Some(bloom), crate::BloomKernel::SeparablePyramid(_)) =
            (self.bloom, self.bloom_kernel)
        {
            crate::gaussian_extents_with_terminal(
                config.size,
                bloom.levels,
                bloom.policies.terminal,
            )?;
        }
        Ok(())
    }

    /// [`Self::validate_frame`] for a renderer whose scene passes use `explicit` jitter.
    ///
    /// `None` means the sequence the renderer derives itself, which is long enough.
    pub fn validate_frame_with_jitter(
        self,
        config: crate::FrameConfig,
        explicit: Option<crate::ProjectionJitter>,
    ) -> Result<(), PostError> {
        self.validate_frame_inner(config)?;
        self.validate_temporal(config, explicit)
    }

    pub fn validate(self) -> Result<(), PostError> {
        self.grading.validate()?;
        self.native_rcas
            .map_or(Ok(()), crate::NativeRcasSettings::validate)?;
        if let Some(upscale) = self.upscale {
            upscale.validate()?;
        }
        if let Some(a) = self.adaptive {
            a.metering.validate()?;
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
                || !(0.0..=b.policies.extraction.threshold_limit()).contains(&b.threshold)
                || !b.knee.is_finite()
                || !(0.0..=1.0).contains(&b.knee)
                || !b.intensity.is_finite()
                || !(0.0..=2.0).contains(&b.intensity)
                || !(1..=8).contains(&b.levels)
        }) {
            return Err(PostError::UnsupportedProfile("bloom settings"));
        }
        self.validate_bloom_policies()?;
        self.resolve.validate()?;
        self.sharpen.map_or(Ok(()), crate::CasSettings::validate)
    }
    fn validate_bloom_policies(self) -> Result<(), PostError> {
        if let Some(bloom) = self.bloom {
            bloom.policies.validate(bloom.levels)?;
            self.bloom_kernel.validate(bloom.levels)?;
            if self.bloom_kernel == crate::BloomKernel::Tent
                && bloom.policies != crate::BloomPolicies::default()
            {
                return Err(PostError::UnsupportedProfile(
                    "bloom policies require separable pyramid",
                ));
            }
        }
        Ok(())
    }
    /// Format of the display-referred image between tone mapping and the later stages.
    ///
    /// Sharpening after no resolve introduces the intermediate, so it takes the narrow
    /// `Rgba16Float`. Every other choice keeps the format its resolve already used.
    pub(crate) fn display_format(self) -> wgpu::TextureFormat {
        match (self.resolve, self.sharpen) {
            (crate::PostResolve::None, Some(_)) => wgpu::TextureFormat::Rgba16Float,
            _ => self.resolve.display_format(),
        }
    }
    pub fn enabled(self) -> bool {
        self.grading != crate::ColorGrading::default()
            || self.tone_map.is_some()
            || self.adaptive.is_some()
            || self.bloom.is_some()
            || self.resolve.requested()
            || self.sharpen.is_some()
            || self.upscale.is_some()
            || self.native_rcas.is_some()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct HdrPost {
    pub settings: HdrPostSettings,
    device: wgpu::Device,
    pub exposure: [wgpu::Buffer; 3],
    carry: wgpu::Buffer,
    frame: [wgpu::Buffer; 3],
    taa: [wgpu::Buffer; 3],
    taa_history: crate::hdr_taa::SharedHistory,
}
impl HdrPost {
    pub fn new(device: &wgpu::Device, settings: &HdrPostSettings) -> Result<Self, PostError> {
        settings.validate()?;
        Ok(Self {
            settings: *settings,
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
            taa: std::array::from_fn(|_| crate::hdr_taa::uniform_buffer(device)),
            taa_history: crate::hdr_taa::SharedHistory::default(),
        })
    }
    /// Rebind settings, keeping buffers. A history that no longer applies is dropped.
    pub fn set_settings(&mut self, settings: &HdrPostSettings) {
        if settings.resolve.active_taa().is_none() {
            self.taa_history.release();
        }
        self.settings = *settings;
    }
    /// Ends a build of every slot: promotes or drops the history bank the build asked for.
    pub fn settle_build(&self, accepted: bool) {
        self.taa_history.settle(accepted);
    }
    /// Uploads the TAA block of `slot`; `None` uploads zeros, which reads as no history.
    pub fn prepare_taa(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        block: Option<&[u8; crate::TAA_UNIFORM_BYTES]>,
    ) {
        crate::hdr_taa::upload(&self.device, encoder, &self.taa[slot], block);
    }
    /// Uploads the temporal block of `slot`; `None` uploads zeros, which reads as no history.
    ///
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] for an invalid block.
    pub fn prepare_temporal(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        slot: usize,
        block: Option<&crate::TemporalBlock>,
    ) -> Result<(), PostError> {
        crate::hdr_taa::upload_temporal(&self.device, encoder, &self.taa[slot], block)
    }
    #[cfg(test)]
    pub fn taa_history_size(&self) -> Option<[u32; 2]> {
        self.taa_history.size()
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
        display: (f64, crate::ToneMap),
    ) -> StageSet {
        let mut set = StageSet::default();
        let exposure = &self.exposure[slot];
        let color = self.settings.resolve.active_taa().map_or_else(
            || targets.resolved().clone(),
            |taa| {
                crate::hdr_taa::append(
                    device,
                    &mut set,
                    targets,
                    (taa, &self.taa_history, &self.taa[slot]),
                    self.settings.reconstructs_temporally(targets.config()),
                )
            },
        );
        let scene = crate::hdr_taa::Scene {
            targets,
            color: &color,
        };
        if let Some(a) = self.settings.adaptive {
            self.meter(&scene, slot, a, &mut set);
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
            &scene,
            &self.settings,
            exposure,
            (format, display.0, display.1),
        );
        set
    }

    fn meter(
        &self,
        scene: &crate::hdr_taa::Scene<'_>,
        slot: usize,
        a: AdaptiveExposure,
        set: &mut StageSet,
    ) {
        let device = &self.device;
        let size = scene.targets.config().size;
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
        let mut constants = exposure_constants(a).to_vec();
        constants.extend(crate::exposure_policy::constants(a));
        set.stages.push(Stage {
            id: crate::StageId::new("clear luminance histogram", 0),
            label: "clear luminance histogram",
            enabled: true,
            operation: crate::Operation::Clear(bins.clone()),
        });
        for (entry, dispatch) in [
            ("histogram", [size[0].div_ceil(8), size[1].div_ceil(8), 1]),
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
            let entries = self.meter_entries(scene.color, slot, &bins, entry);
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
        scene_color: &'a wgpu::TextureView,
        slot: usize,
        bins: &'a wgpu::Buffer,
        entry: &str,
    ) -> Vec<wgpu::BindGroupEntry<'a>> {
        if entry == "histogram" {
            vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(scene_color),
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
    if let crate::ExposureMetering::InterpolatedPercentile(p) = a.metering {
        let l = crate::percentile_luminance(bins, a.log_range, p.percentile);
        let key = (1.03 - 2.0 / (2.0 + (l + 1.0).log10())) * f64::from(p.key_bias);
        return (key / l).max(f64::from(a.exposure_range[0]));
    }
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
