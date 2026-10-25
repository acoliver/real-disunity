use crate::{Bloom, GaussianLevel, Stage, StageId, StageSet, StageTarget, color_texture};
use wgpu::util::DeviceExt;

pub(crate) struct GaussianPyramid<'a> {
    device: &'a wgpu::Device,
    set: &'a mut StageSet,
    module: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
}

#[derive(Clone, Copy)]
struct Pass<'a> {
    entry: &'static str,
    level: u32,
    source: &'a wgpu::TextureView,
    lower: Option<&'a wgpu::TextureView>,
    weights: Option<&'a wgpu::Buffer>,
    exposure: Option<&'a wgpu::Buffer>,
    filtered: bool,
    target: &'a wgpu::TextureView,
    constants: &'a [(&'a str, f64)],
}

impl<'a> GaussianPyramid<'a> {
    pub fn new(device: &'a wgpu::Device, set: &'a mut StageSet) -> Self {
        Self {
            device,
            set,
            module: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("separable Gaussian pyramid"),
                source: wgpu::ShaderSource::Wgsl(include_str!("gaussian.wgsl").into()),
            }),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Gaussian bilinear clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
        }
    }

    pub fn build(
        &mut self,
        source: &wgpu::TextureView,
        bloom: Bloom,
        levels: &[GaussianLevel; 8],
        exposure: Option<(&wgpu::Buffer, f64)>,
    ) -> wgpu::TextureView {
        let extents = crate::gaussian_extents(
            [source.texture().width(), source.texture().height()],
            bloom.levels,
        )
        .expect("validated Gaussian extents");
        let mut pyramid = Vec::new();
        let mut source = source.clone();
        for (index, size) in extents.into_iter().enumerate() {
            let target = self.texture(size);
            self.reduce_level(index, &source, &target, bloom, exposure);
            source = target.clone();
            pyramid.push(target);
        }
        let mut lower = None;
        for (index, target) in
            pyramid.iter().enumerate().rev().take(
                usize::try_from(bloom.levels - bloom.policies.selection.first_blurred).unwrap(),
            )
        {
            let scratch = self.texture([target.texture().width(), target.texture().height()]);
            self.blur_level(
                (index, levels[index], bloom.policies),
                target,
                &scratch,
                lower.as_ref(),
            );
            lower = Some(target.clone());
        }
        pyramid[usize::try_from(bloom.policies.selection.output).unwrap()].clone()
    }

    fn reduce_level(
        &mut self,
        index: usize,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        bloom: Bloom,
        exposure: Option<(&wgpu::Buffer, f64)>,
    ) {
        let mut constants = super::bloom_builder::bright_constants(bloom);
        constants.push(("HOST_EXPOSURE", exposure.map_or(1.0, |(_, scale)| scale)));
        let filtered = bloom.policies.reduction == crate::BloomReduction::QuarterTexelAfterAverage;
        let tone = matches!(
            bloom.policies.extraction,
            crate::BrightExtraction::ToneCurve(_)
        );
        let entry = match (index == 0, filtered, tone) {
            (true, _, true) => "gaussian_tone_bright",
            (true, true, false) => "gaussian_filtered_bright",
            (false, true, _) => "gaussian_filtered_downsample",
            (true, false, false) => "gaussian_bright",
            (false, false, _) => "gaussian_downsample",
        };
        self.pass(Pass {
            entry,
            level: u32::try_from(index).unwrap(),
            source,
            target,
            lower: None,
            weights: None,
            exposure: if index == 0 && tone {
                exposure.map(|(buffer, _)| buffer)
            } else {
                None
            },
            filtered: filtered || (index == 0 && tone),
            constants: &constants,
        });
    }

    fn blur_level(
        &mut self,
        config: (usize, GaussianLevel, crate::BloomPolicies),
        target: &wgpu::TextureView,
        scratch: &wgpu::TextureView,
        lower: Option<&wgpu::TextureView>,
    ) {
        let (index, level, policies) = config;
        let weights_for = |extent| {
            policies
                .kernel
                .weights(level.radius, extent)
                .expect("validated Gaussian radius")
        };
        let horizontal = self.weight_buffer(&weights_for(target.texture().width()));
        let vertical = (policies.kernel != crate::GaussianKernelRule::RadiusThird)
            .then(|| self.weight_buffer(&weights_for(target.texture().height())));
        for (entry, extent, source, destination) in [
            (
                "gaussian_horizontal",
                target.texture().width(),
                target,
                scratch,
            ),
            (
                "gaussian_vertical",
                target.texture().height(),
                scratch,
                target,
            ),
        ] {
            let weights = policies
                .kernel
                .weights(level.radius, extent)
                .expect("validated Gaussian radius");
            let buffer = if entry == "gaussian_vertical" {
                vertical.as_ref().unwrap_or(&horizontal)
            } else {
                &horizontal
            };
            let constants = blur_constants(
                level,
                weights.taps.len(),
                policies.tint_pass,
                lower.is_some(),
            );
            self.pass(Pass {
                entry,
                level: u32::try_from(index).unwrap(),
                source,
                lower,
                weights: Some(buffer),
                exposure: None,
                filtered: true,
                target: destination,
                constants: &constants,
            });
        }
        self.set.resources.buffers.push(horizontal);
        self.set.resources.buffers.extend(vertical);
    }

    fn weight_buffer(&self, weights: &crate::GaussianWeights) -> wgpu::Buffer {
        let mut data = vec![0.0_f32; crate::gaussian::GAUSSIAN_TAP_CAPACITY * 2];
        data[1] = weights.center;
        for (pair, tap) in data[2..].chunks_exact_mut(2).zip(&weights.taps) {
            pair.copy_from_slice(&[tap.offset, tap.weight]);
        }
        let bytes: Vec<_> = data.into_iter().flat_map(f32::to_le_bytes).collect();
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gaussian folded weights"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            })
    }

    fn texture(&mut self, size: [u32; 2]) -> wgpu::TextureView {
        let texture = color_texture(
            self.device,
            "Gaussian level",
            size,
            wgpu::TextureFormat::Rgba16Float,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.set.resources.textures.push(texture);
        view
    }

    fn pipeline(&self, p: &Pass<'_>) -> wgpu::RenderPipeline {
        self.device
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
                    targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
    }

    fn pass(&mut self, p: Pass<'_>) {
        let pipeline = self.pipeline(&p);
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(p.source),
        }];
        if p.filtered {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            });
        }
        if let Some(buffer) = p.weights {
            entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: buffer.as_entire_binding(),
            });
        }
        if let Some(buffer) = p.exposure {
            entries.push(wgpu::BindGroupEntry {
                binding: 4,
                resource: buffer.as_entire_binding(),
            });
        }
        // The lower binding is live only for the vertical entry, including its terminal level.
        if p.entry == "gaussian_vertical" {
            entries.push(wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(p.lower.unwrap_or(p.source)),
            });
        }
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(p.entry),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        self.set.stages.push(Stage::fullscreen(
            StageId::new(p.entry, p.level),
            &pipeline,
            &[&group],
            StageTarget::Texture(p.target.clone()),
        ));
    }
}

fn blur_constants(
    level: GaussianLevel,
    tap_count: usize,
    tint_pass: crate::BloomTintPass,
    fold: bool,
) -> [(&'static str, f64); 7] {
    [
        ("TAPS", f64::from(u32::try_from(tap_count).unwrap())),
        ("TINT_R", level.tint[0].into()),
        ("TINT_G", level.tint[1].into()),
        ("TINT_B", level.tint[2].into()),
        ("WEIGHT", level.weight.into()),
        (
            "HORIZONTAL_TINT",
            f64::from(u8::from(tint_pass == crate::BloomTintPass::Horizontal)),
        ),
        ("FOLD", f64::from(u8::from(fold))),
    ]
}
