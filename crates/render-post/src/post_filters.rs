use crate::hdr_taa::Scene;
use crate::{FrameTargets, HdrPostSettings, Stage, StageSet, StageTarget, color_texture};

pub(crate) struct Filters<'a> {
    pub device: &'a wgpu::Device,
    pub set: &'a mut StageSet,
    module: wgpu::ShaderModule,
}
/// The stages that run on the display-referred image after tone mapping.
#[derive(Clone, Copy)]
struct LaterStages {
    resolve: crate::PostResolve,
    sharpen: Option<crate::CasSettings>,
    upscale: Option<crate::UpscaleSettings>,
    native_rcas: Option<crate::NativeRcasSettings>,
}
struct FilterPass<'a> {
    entry: &'static str,
    level: u32,
    source: &'a wgpu::TextureView,
    lower: Option<&'a wgpu::TextureView>,
    exposure: Option<&'a wgpu::Buffer>,
    constants: &'a [(&'a str, f64)],
    format: wgpu::TextureFormat,
    target: StageTarget,
}
impl<'a> Filters<'a> {
    pub fn new(device: &'a wgpu::Device, set: &'a mut StageSet) -> Self {
        Self {
            device,
            set,
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("HDR post filters"),
                source: wgpu::ShaderSource::Wgsl(crate::aces::shader(include_str!(
                    "post_filters.wgsl"
                ))),
            }),
        }
    }
    pub fn build(
        &mut self,
        scene: &Scene<'_>,
        settings: &HdrPostSettings,
        exposure: &wgpu::Buffer,
        output: (wgpu::TextureFormat, f64, crate::ToneMap),
    ) {
        let targets = scene.targets;
        let bloom = self.bloom_source(scene, settings, exposure, output.1);
        let constants = tone_constants(settings, output.1, output.2);
        let resolve = settings.resolve;
        let upscale = settings.upscale.filter(|_| {
            targets.config().size != targets.config().output_extent()
                && !settings.reconstructs_temporally(targets.config())
        });
        let native_rcas = settings.native_rcas.filter(|_| upscale.is_none());
        let resolved = resolve.schedules_pass();
        let display_format = if native_rcas.is_some() && !resolved && settings.sharpen.is_none() {
            wgpu::TextureFormat::Rgba32Float
        } else {
            settings.display_format()
        };
        let ldr =
            (resolved || settings.sharpen.is_some() || upscale.is_some() || native_rcas.is_some())
                .then(|| self.display_texture(targets.config().size, display_format));
        self.pass(FilterPass {
            entry: "tone",
            level: 0,
            source: scene.color,
            lower: Some(&bloom),
            exposure: Some(exposure),
            constants: &constants,
            format: ldr.as_ref().map_or(output.0, |_| display_format),
            target: ldr
                .as_ref()
                .map_or(StageTarget::Output, |v| StageTarget::Texture(v.clone())),
        });
        let later = LaterStages {
            resolve,
            sharpen: settings.sharpen,
            upscale,
            native_rcas,
        };
        self.finish(targets, later, ldr.as_ref(), output.0);
    }
    /// Resolve, sharpen and upscale, in that order. Each stage reads the one before it and
    /// the last writes the output; an intermediate exists only between two scheduled stages.
    fn finish(
        &mut self,
        targets: &FrameTargets,
        later: LaterStages,
        ldr: Option<&wgpu::TextureView>,
        format: wgpu::TextureFormat,
    ) {
        let size = targets.config().size;
        let resolving = later.resolve.schedules_pass();
        let sharpening = later.sharpen.is_some();
        let final_filter = later.upscale.is_some() || later.native_rcas.is_some();
        // A resolve feeding only an upscale keeps its Rgba32Float intermediate. One feeding
        // sharpening writes the new, narrower Rgba16Float intermediate.
        let resolve_format = if sharpening {
            wgpu::TextureFormat::Rgba16Float
        } else {
            wgpu::TextureFormat::Rgba32Float
        };
        let resolved = (resolving && (sharpening || final_filter))
            .then(|| self.display_texture(size, resolve_format));
        let sharpen_format = if later.native_rcas.is_some() {
            wgpu::TextureFormat::Rgba32Float
        } else {
            wgpu::TextureFormat::Rgba16Float
        };
        let sharpened =
            (sharpening && final_filter).then(|| self.display_texture(size, sharpen_format));
        if resolving {
            let output = resolved.as_ref().map_or_else(
                || (format, StageTarget::Output),
                |v| (resolve_format, StageTarget::Texture(v.clone())),
            );
            self.resolve_pass(later.resolve, ldr.unwrap(), size, output);
        }
        if let Some(cas) = later.sharpen {
            let output = sharpened.as_ref().map_or_else(
                || (format, StageTarget::Output),
                |v| (sharpen_format, StageTarget::Texture(v.clone())),
            );
            let source = resolved.as_ref().or(ldr).unwrap();
            crate::cas::append(self.device, self.set, source, cas, output);
        }
        if final_filter {
            self.final_filter(
                targets,
                later,
                sharpened.as_ref().or(resolved.as_ref()).or(ldr).unwrap(),
                format,
            );
        }
    }
    fn final_filter(
        &mut self,
        targets: &FrameTargets,
        later: LaterStages,
        source: &wgpu::TextureView,
        format: wgpu::TextureFormat,
    ) {
        if let Some(settings) = later.native_rcas {
            crate::upscale::append_native(self.device, self.set, source, settings, format);
        }
        if let Some(settings) = later.upscale {
            crate::upscale::append(
                self.device,
                self.set,
                source,
                targets.config().output_extent(),
                &crate::SpatialUpscale {
                    settings,
                    output_format: format,
                },
            );
        }
    }
    fn resolve_pass(
        &mut self,
        resolve: crate::PostResolve,
        source: &wgpu::TextureView,
        size: [u32; 2],
        output: (wgpu::TextureFormat, StageTarget),
    ) {
        if let Some(crate::FxaaSettings { strength, quality }) = resolve.active_fxaa() {
            self.pass(FilterPass {
                entry: "fxaa",
                level: 0,
                source,
                lower: None,
                exposure: None,
                constants: &quality.constants(strength),
                format: output.0,
                target: output.1,
            });
        } else if let Some(smaa) = resolve.active_smaa() {
            crate::smaa_gpu::append(self.device, self.set, source, size, smaa, output);
        } else if let Some(cmaa2) = resolve.active_cmaa2() {
            crate::cmaa2_gpu::append(self.device, self.set, source, size, cmaa2, output);
        }
    }
    fn display_texture(
        &mut self,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> wgpu::TextureView {
        let texture = color_texture(self.device, "display-linear post input", size, format);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.set.resources.textures.push(texture);
        view
    }
    fn bloom_source(
        &mut self,
        scene: &Scene<'_>,
        settings: &HdrPostSettings,
        exposure: &wgpu::Buffer,
        fixed: f64,
    ) -> wgpu::TextureView {
        settings.bloom.map_or_else(
            || scene.color.clone(),
            |b| match settings.bloom_kernel {
                crate::BloomKernel::Tent => self.bloom(scene, b),
                crate::BloomKernel::SeparablePyramid(levels) => {
                    let fixed_buffer = (settings.adaptive.is_none()
                        && matches!(b.policies.extraction, crate::BrightExtraction::ToneCurve(_)))
                    .then(|| {
                        use wgpu::util::DeviceExt;
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("stock fixed bloom exposure"),
                                contents: &1.0_f32.to_le_bytes(),
                                usage: wgpu::BufferUsages::STORAGE,
                            })
                    });
                    let view = crate::gaussian_gpu::GaussianPyramid::new(self.device, self.set)
                        .build(
                            scene.color,
                            b,
                            &levels,
                            Some((
                                fixed_buffer.as_ref().unwrap_or(exposure),
                                if fixed_buffer.is_some() { fixed } else { 1.0 },
                            )),
                        );
                    if let Some(buffer) = fixed_buffer {
                        self.set.resources.buffers.push(buffer);
                    }
                    view
                }
            },
        )
    }
    fn bloom(&mut self, scene: &Scene<'_>, b: crate::Bloom) -> wgpu::TextureView {
        let mut levels = Vec::new();
        let mut size = scene.targets.config().size;
        let first = self.texture(size);
        self.filter(
            ("prefilter", 0),
            scene.color,
            None,
            &[("THRESHOLD", b.threshold.into()), ("KNEE", b.knee.into())],
            &first,
        );
        levels.push(first);
        for level in 1..b.levels {
            if size == [1, 1] {
                break;
            }
            size = [size[0].div_ceil(2), size[1].div_ceil(2)];
            let next = self.texture(size);
            self.filter(
                ("downsample", level),
                levels.last().unwrap(),
                None,
                &[],
                &next,
            );
            levels.push(next);
        }
        let mut bloom = levels.pop().unwrap();
        for (level, high) in levels.iter().enumerate().rev() {
            let next = self.texture([high.texture().width(), high.texture().height()]);
            self.filter(
                ("upsample", u32::try_from(level).unwrap()),
                high,
                Some(&bloom),
                &[],
                &next,
            );
            bloom = next;
        }
        bloom
    }
    fn filter(
        &mut self,
        id: (&'static str, u32),
        source: &wgpu::TextureView,
        lower: Option<&wgpu::TextureView>,
        constants: &[(&str, f64)],
        target: &wgpu::TextureView,
    ) {
        self.pass(FilterPass {
            entry: id.0,
            level: id.1,
            source,
            lower,
            exposure: None,
            constants,
            format: wgpu::TextureFormat::Rgba16Float,
            target: StageTarget::Texture(target.clone()),
        });
    }
    fn texture(&mut self, size: [u32; 2]) -> wgpu::TextureView {
        let texture = color_texture(
            self.device,
            "HDR post intermediate",
            size,
            wgpu::TextureFormat::Rgba16Float,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.set.resources.textures.push(texture);
        view
    }
    fn pass(&mut self, p: FilterPass<'_>) {
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(p.entry),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &self.module,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some(p.entry),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: p.constants,
                        ..Default::default()
                    },
                    targets: &[Some(p.format.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(p.source),
        }];
        if let Some(lower) = p.lower {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(lower),
            });
        }
        if let Some(exposure) = p.exposure {
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: exposure.as_entire_binding(),
            });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(p.entry),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        self.set.stages.push(Stage::fullscreen(
            crate::StageId::new(p.entry, p.level),
            &pipeline,
            &[&group],
            p.target,
        ));
    }
}

fn tone_constants(
    settings: &HdrPostSettings,
    exposure: f64,
    tone_map: crate::ToneMap,
) -> Vec<(&'static str, f64)> {
    let mut constants = vec![
        (
            "GAUSSIAN",
            f64::from(u8::from(matches!(
                settings.bloom_kernel,
                crate::BloomKernel::SeparablePyramid(_)
            ))),
        ),
        ("FIXED", exposure),
        ("CURVE", tone_map.constant()),
        ("ADAPTIVE", f64::from(u8::from(settings.adaptive.is_some()))),
        (
            "INTENSITY",
            settings.bloom.map_or(0.0, |b| f64::from(b.intensity)),
        ),
    ];
    constants.extend(crate::exposure_policy::output_constants(settings.adaptive));
    constants.extend(settings.grading.constants());
    constants.push((
        "GRADE_COMPOSITION",
        f64::from(u8::from(settings.grading != crate::ColorGrading::default())),
    ));
    constants
}
