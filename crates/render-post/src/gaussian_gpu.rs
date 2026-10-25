use crate::{
    Bloom, FrameTargets, GaussianLevel, Stage, StageId, StageSet, StageTarget, color_texture,
};
use wgpu::util::DeviceExt;

pub(crate) struct GaussianPyramid<'a> {
    device: &'a wgpu::Device,
    set: &'a mut StageSet,
    module: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    by_value: bool,
}

#[derive(Clone, Copy)]
struct Pass<'a> {
    entry: &'static str,
    level: u32,
    source: &'a wgpu::TextureView,
    lower: Option<&'a wgpu::TextureView>,
    weights: Option<&'a wgpu::Buffer>,
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
            by_value: false,
        }
    }

    #[cfg(test)]
    pub fn use_by_value(&mut self) {
        self.by_value = true;
    }

    pub fn build(
        &mut self,
        targets: &FrameTargets,
        bloom: Bloom,
        levels: &[GaussianLevel; 8],
    ) -> wgpu::TextureView {
        let extents = crate::gaussian_extents(targets.config().size, bloom.levels)
            .expect("validated Gaussian extents");
        let mut pyramid = Vec::new();
        let mut source = targets.resolved().clone();
        for (index, size) in extents.into_iter().enumerate() {
            let target = self.texture(size);
            let entry = if index == 0 {
                "gaussian_bright"
            } else {
                "gaussian_downsample"
            };
            self.pass(Pass {
                entry,
                level: u32::try_from(index).unwrap(),
                source: &source,
                lower: None,
                weights: None,
                target: &target,
                constants: &[
                    ("THRESHOLD", bloom.threshold.into()),
                    ("KNEE", bloom.knee.into()),
                ],
            });
            source = target.clone();
            pyramid.push(target);
        }
        let mut lower = None;
        for (index, target) in pyramid.iter().enumerate().rev() {
            let scratch = self.texture([target.texture().width(), target.texture().height()]);
            self.blur_level(index, levels[index], target, &scratch, lower.as_ref());
            lower = Some(target.clone());
        }
        pyramid[0].clone()
    }

    fn blur_level(
        &mut self,
        index: usize,
        level: GaussianLevel,
        target: &wgpu::TextureView,
        scratch: &wgpu::TextureView,
        lower: Option<&wgpu::TextureView>,
    ) {
        let weights = crate::gaussian_weights(level.radius).expect("validated Gaussian radius");
        let mut data = vec![0.0_f32; crate::gaussian::GAUSSIAN_TAP_CAPACITY * 2];
        data[1] = weights.center;
        for (pair, tap) in data[2..].chunks_exact_mut(2).zip(&weights.taps) {
            pair.copy_from_slice(&[tap.offset, tap.weight]);
        }
        let bytes: Vec<_> = data.into_iter().flat_map(f32::to_le_bytes).collect();
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Gaussian folded weights"),
                contents: &bytes,
                usage: wgpu::BufferUsages::STORAGE,
            });
        let constants = [
            (
                "TAPS",
                f64::from(u32::try_from(weights.taps.len()).unwrap()),
            ),
            ("TINT_R", level.tint[0].into()),
            ("TINT_G", level.tint[1].into()),
            ("TINT_B", level.tint[2].into()),
            ("WEIGHT", level.weight.into()),
            ("FOLD", f64::from(u8::from(lower.is_some()))),
            ("BY_VALUE", f64::from(u8::from(self.by_value))),
        ];
        self.pass(Pass {
            entry: "gaussian_horizontal",
            level: u32::try_from(index).unwrap(),
            source: target,
            lower: None,
            weights: Some(&buffer),
            target: scratch,
            constants: &constants,
        });
        self.pass(Pass {
            entry: "gaussian_vertical",
            level: u32::try_from(index).unwrap(),
            source: scratch,
            lower,
            weights: Some(&buffer),
            target,
            constants: &constants,
        });
        self.set.resources.buffers.push(buffer);
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

    fn pass(&mut self, p: Pass<'_>) {
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
                    targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
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
        if let Some(buffer) = p.weights {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 3,
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
