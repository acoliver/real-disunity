use super::*;

pub(super) fn gpu() -> (wgpu::Device, wgpu::Queue) {
    gpu_with_optional_features(wgpu::Features::empty())
}

pub(super) fn gpu_with_optional_features(optional: wgpu::Features) -> (wgpu::Device, wgpu::Queue) {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle().with_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        descriptor.backends = wgpu::Backends::PRIMARY;
    }
    let instance = wgpu::Instance::new(descriptor);
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("post service tests require a PRIMARY GPU; WGPU_BACKEND can override");
    eprintln!("post service adapter: {:?}", adapter.get_info());
    let required_features = optional & adapter.features();
    eprintln!("post service enabled features: {required_features:?}");
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features,
        ..Default::default()
    }))
    .unwrap()
}

fn clear(encoder: &mut wgpu::CommandEncoder, targets: &FrameTargets) {
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: targets.scene(),
            resolve_target: targets.resolve(),
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.25,
                    g: 0.5,
                    b: 1.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}

fn render<F: StageFactory>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<F>,
) -> Vec<u8> {
    let config = FrameConfig {
        samples: 1,
        format: wgpu::TextureFormat::Rgba8Unorm,
        ..chain.targets().config()
    };
    let output = color_texture(device, "post GPU output", config.size, config.format);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    clear(&mut encoder, chain.targets());
    chain.encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    Readback::new(device, config, 1024 * 1024)
        .unwrap()
        .read(device, queue, &output)
        .unwrap()
}

#[test]
fn copy_only_is_exact_with_msaa_hdr_resolve_and_unaligned_readback() {
    let (device, queue) = gpu();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba16Float,
    ] {
        for samples in [1, 4] {
            let config = FrameConfig::new([17, 9], format, samples);
            let chain = PostChain::new(
                &device,
                config,
                Resources::default(),
                CopyOutput::new(&device, wgpu::TextureFormat::Rgba8Unorm),
            )
            .unwrap();
            assert_eq!(chain.targets().resolve().is_some(), samples > 1);
            let pixels = render(&device, &queue, &chain);
            assert_eq!(pixels.len(), 17 * 9 * 4);
            for pixel in pixels.chunks_exact(4) {
                assert_eq!(pixel, [64, 128, 255, 255]);
            }
        }
    }
}

/// A caller-authored history ABI, deliberately different from any renderer's exposure layout.
#[derive(Clone)]
struct AuthoredEffect {
    enabled: bool,
    copy: CopyOutput,
    compute: wgpu::ComputePipeline,
    display: wgpu::RenderPipeline,
}

impl AuthoredEffect {
    fn new(device: &wgpu::Device) -> Self {
        let compute = Self::compute(device);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authored test display"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> history: vec4<u32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    return vec4<f32>(f32((i << 1u) & 2u) * 2.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(textureLoad(image, vec2<i32>(p.xy), 0).rgb * (f32(history.x) / 8.0), 1.0);
}
"
                .into(),
            ),
        });
        let display = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Self {
            enabled: false,
            copy: CopyOutput::new(device, wgpu::TextureFormat::Rgba8Unorm),
            compute,
            display,
        }
    }
    fn compute(device: &wgpu::Device) -> wgpu::ComputePipeline {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("authored test history ABI"),
            source: wgpu::ShaderSource::Wgsl(
                r"
@group(0) @binding(0) var<storage, read_write> history: vec4<u32>;
@compute @workgroup_size(1) fn step() { history.x += 1u; }
"
                .into(),
            ),
        });
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some("step"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    }
}

impl StageFactory for AuthoredEffect {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        persistent: &Resources,
    ) -> Result<StageSet, PostError> {
        if !self.enabled {
            let mut copy = self.copy.build(device, targets, persistent)?;
            copy.stages.insert(
                0,
                Stage {
                    id: crate::StageId::new("disabled history reset", 0),
                    label: "disabled history reset",
                    enabled: false,
                    operation: Operation::Clear(persistent.buffers[0].clone()),
                },
            );
            return Ok(copy);
        }
        let history = &persistent.buffers[0];
        let compute = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.compute.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: history.as_entire_binding(),
            }],
        });
        let display = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.display.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(targets.resolved()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: history.as_entire_binding(),
                },
            ],
        });
        Ok(StageSet {
            stages: vec![
                Stage {
                    id: crate::StageId::new("authored history update", 0),
                    label: "authored history update",
                    enabled: true,
                    operation: Operation::Compute {
                        pipeline: self.compute.clone(),
                        groups: vec![compute],
                        dispatch: [1, 1, 1],
                    },
                },
                Stage::fullscreen(
                    crate::StageId::new("authored output", 0),
                    &self.display,
                    &[&display],
                    StageTarget::Output,
                ),
            ],
            resources: Resources::default(),
        })
    }
}

#[test]
fn configured_optional_effect_changes_pixels_and_resize_preserves_fixed_history() {
    let (device, queue) = gpu();
    let resources = Resources::new(
        &device,
        &[BufferSpec {
            label: "authored history",
            size: 16,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::UNIFORM
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            initial: &[0; 16],
        }],
    )
    .unwrap();
    let mut effect = AuthoredEffect::new(&device);
    let mut chain = PostChain::new(
        &device,
        FrameConfig::new([17, 9], wgpu::TextureFormat::Rgba16Float, 1),
        resources,
        effect.clone(),
    )
    .unwrap();
    let disabled = render(&device, &queue, &chain);
    assert_eq!(&disabled[..4], [64, 128, 255, 255]);
    effect.enabled = true;
    chain.reconfigure(&device, effect.clone()).unwrap();
    assert_eq!(&render(&device, &queue, &chain)[..4], [8, 16, 32, 255]);
    assert!(chain.resize(&device, [13, 7]).unwrap());
    assert!(!chain.resize(&device, [13, 7]).unwrap());
    assert_eq!(&render(&device, &queue, &chain)[..4], [16, 32, 64, 255]);
    assert_eq!(chain.resize(&device, [0, 8]), Err(PostError::EmptyExtent));
    assert_eq!(chain.targets().config().size, [13, 7]);
    assert_eq!(&render(&device, &queue, &chain)[..4], [24, 48, 96, 255]);
    effect.enabled = false;
    chain.reconfigure(&device, effect.clone()).unwrap();
    assert_eq!(&render(&device, &queue, &chain)[..4], [64, 128, 255, 255]);
    effect.enabled = true;
    chain.reconfigure(&device, effect).unwrap();
    assert_eq!(&render(&device, &queue, &chain)[..4], [32, 64, 128, 255]);
}

struct InvalidStages;
impl StageFactory for InvalidStages {
    fn build(
        &self,
        _: &wgpu::Device,
        _: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        Ok(StageSet::default())
    }
}

#[test]
fn invalid_resources_and_missing_output_fail_before_encoding() {
    let (device, _) = gpu();
    let config = FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1);
    assert!(matches!(
        PostChain::new(&device, config, Resources::default(), InvalidStages),
        Err(PostError::OutputStage)
    ));
    assert!(matches!(
        Resources::new(
            &device,
            &[BufferSpec {
                label: "bad",
                size: 3,
                usage: wgpu::BufferUsages::UNIFORM,
                initial: &[]
            }]
        ),
        Err(PostError::BufferSize(3))
    ));
    assert!(matches!(
        Readback::new(&device, config, 1),
        Err(ReadbackError::Limit)
    ));
}
