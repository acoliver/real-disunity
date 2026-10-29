use crate::{FrameTargets, HdrPostSettings, Stage, StageSet, StageTarget, color_texture};

pub(crate) struct Filters<'a> {
    pub device: &'a wgpu::Device,
    pub set: &'a mut StageSet,
    module: wgpu::ShaderModule,
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
        targets: &FrameTargets,
        settings: &HdrPostSettings,
        exposure: &wgpu::Buffer,
        output: (wgpu::TextureFormat, f64, crate::ToneMap),
    ) {
        let bloom = self.bloom_source(targets, settings, exposure, output.1);
        let mut constants = vec![
            (
                "GAUSSIAN",
                f64::from(u8::from(matches!(
                    settings.bloom_kernel,
                    crate::BloomKernel::SeparablePyramid(_)
                ))),
            ),
            ("FIXED", output.1),
            ("CURVE", output.2.constant()),
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
        let resolve = settings.resolve;
        let upscale = settings
            .upscale
            .filter(|_| targets.config().size != targets.config().output_extent());
        let resolved = resolve.schedules_pass();
        let display_format = resolve.display_format();
        let ldr = (resolved || upscale.is_some())
            .then(|| self.display_texture(targets.config().size, display_format));
        self.pass(FilterPass {
            entry: "tone",
            level: 0,
            source: targets.resolved(),
            lower: Some(&bloom),
            exposure: Some(exposure),
            constants: &constants,
            format: ldr.as_ref().map_or(output.0, |_| display_format),
            target: ldr
                .as_ref()
                .map_or(StageTarget::Output, |v| StageTarget::Texture(v.clone())),
        });
        self.finish(targets, resolve, upscale, ldr.as_ref(), output.0);
    }
    fn finish(
        &mut self,
        targets: &FrameTargets,
        resolve: crate::PostResolve,
        upscale: Option<crate::UpscaleSettings>,
        ldr: Option<&wgpu::TextureView>,
        format: wgpu::TextureFormat,
    ) {
        let resolving = resolve.schedules_pass();
        let filtered = (resolving && upscale.is_some())
            .then(|| self.display_texture(targets.config().size, wgpu::TextureFormat::Rgba32Float));
        let (resolve_format, resolve_target) = filtered.as_ref().map_or_else(
            || (format, StageTarget::Output),
            |v| {
                (
                    wgpu::TextureFormat::Rgba32Float,
                    StageTarget::Texture(v.clone()),
                )
            },
        );
        if let Some(crate::FxaaSettings { strength, quality }) = resolve.active_fxaa() {
            self.pass(FilterPass {
                entry: "fxaa",
                level: 0,
                source: ldr.unwrap(),
                lower: None,
                exposure: None,
                constants: &quality.constants(strength),
                format: resolve_format,
                target: resolve_target,
            });
        } else if let Some(smaa) = resolve.active_smaa() {
            crate::smaa_gpu::append(
                self.device,
                self.set,
                ldr.unwrap(),
                targets.config().size,
                smaa,
                (resolve_format, resolve_target),
            );
        } else if let Some(cmaa2) = resolve.active_cmaa2() {
            crate::cmaa2_gpu::append(
                self.device,
                self.set,
                ldr.unwrap(),
                targets.config().size,
                cmaa2,
                (resolve_format, resolve_target),
            );
        }
        if let Some(settings) = upscale {
            crate::upscale::append(
                self.device,
                self.set,
                filtered.as_ref().or(ldr).unwrap(),
                targets.config().output_extent(),
                &crate::SpatialUpscale {
                    settings,
                    output_format: format,
                },
            );
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
        targets: &FrameTargets,
        settings: &HdrPostSettings,
        exposure: &wgpu::Buffer,
        fixed: f64,
    ) -> wgpu::TextureView {
        settings.bloom.map_or_else(
            || targets.resolved().clone(),
            |b| match settings.bloom_kernel {
                crate::BloomKernel::Tent => self.bloom(targets, b),
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
                            targets.resolved(),
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
    fn bloom(&mut self, targets: &FrameTargets, b: crate::Bloom) -> wgpu::TextureView {
        let mut levels = Vec::new();
        let mut size = targets.config().size;
        let first = self.texture(size);
        self.filter(
            ("prefilter", 0),
            targets.resolved(),
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
