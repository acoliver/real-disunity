//! Test rig of the temporal stage on lavapipe for analytic scenes: a WGSL scene shader that
//! writes colour, backward motion and device depth at the internal extent is rasterised for
//! each frame of the jitter sequence, the temporal chain resolves it, and the HDR output is read
//! back. The scene shader reads a uniform of the internal extent, the jitter in internal pixels
//! and four scene values (`tail`).
use crate::hdr_temporal_build_tests::{public_factory, settings};
use crate::taa_gpu_test_support::read;
use crate::temporal_quality_tests::Recording;
use crate::{
    FrameConfig, PostChain, ProjectionJitter, Resources, TaaSettings, TemporalBlock, TemporalCamera,
};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

pub(crate) const OUTPUT: [u32; 2] = [160, 96];

pub(crate) struct SceneRig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<Recording>,
    hdr: wgpu::Texture,
    sink: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    render: [u32; 2],
    pub(crate) jitter: ProjectionJitter,
}

fn pipeline(device: &wgpu::Device, source: String) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("analytic scene"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("analytic scene"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("paint"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[
                Some(wgpu::TextureFormat::Rgba16Float.into()),
                Some(crate::VELOCITY_FORMAT.into()),
            ],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

impl SceneRig {
    pub(crate) fn new(
        gpu: &(wgpu::Device, wgpu::Queue),
        render: [u32; 2],
        scene_wgsl: String,
    ) -> Self {
        let (device, queue) = gpu.clone();
        let config = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let output = Arc::new(Mutex::new(None));
        let factory = Recording {
            inner: public_factory(&device, &settings()),
            output: output.clone(),
        };
        let chain = PostChain::new(&device, config, Resources::default(), factory).unwrap();
        let hdr = output.lock().unwrap().take().expect("slot 0 was built");
        let sink =
            crate::color_texture(&device, "sink", OUTPUT, wgpu::TextureFormat::Rgba8UnormSrgb);
        Self {
            pipeline: pipeline(&device, scene_wgsl),
            device,
            queue,
            chain,
            hdr,
            sink,
            render,
            jitter: ProjectionJitter::for_extents(render, OUTPUT).unwrap(),
        }
    }

    /// Rasterises the scene into colour, velocity and depth at the internal extent.
    fn paint(&self, encoder: &mut wgpu::CommandEncoder, offset: [f32; 2], tail: [f32; 4]) {
        let [rw, rh] = self
            .render
            .map(|side| f32::from(u16::try_from(side).unwrap()));
        let values = [
            rw, rh, offset[0], offset[1], tail[0], tail[1], tail[2], tail[3],
        ];
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene frame"),
                contents: &bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let targets = self.chain.targets();
        let attachment = |view| {
            Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("analytic scene"),
            color_attachments: &[
                attachment(targets.resolved()),
                attachment(targets.velocity().expect("temporal needs velocity")),
            ],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: targets.depth(),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Frame `index` of the jitter sequence with the scene values `tail`. The camera term is
    /// uploaded when given, otherwise the slot keeps the zeroed block of a stage without one.
    pub(crate) fn frame(
        &self,
        index: u64,
        history_valid: bool,
        (tail, camera): ([f32; 4], Option<TemporalCamera>),
    ) {
        let offset = self.jitter.offset_pixels(index);
        let block = TemporalBlock {
            render: self.render,
            output: OUTPUT,
            jitter: offset,
            phase_count: self.jitter.length(),
            settings: TaaSettings::new(0.9),
            history_valid,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.paint(&mut encoder, offset, tail);
        let factory = &self.chain.factory().inner;
        if let Some(camera) = camera {
            factory.prepare_temporal_camera(&mut encoder, 0, camera);
        }
        factory.prepare_temporal(&mut encoder, 0, &block).unwrap();
        factory
            .prepare_effects(&mut encoder, 0, 0.031, true)
            .unwrap();
        let view = self
            .sink
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.chain.encode(&mut encoder, &view);
        self.queue.submit([encoder.finish()]);
    }

    /// The HDR reconstruction of the last frame, mean of the colour channels, row major.
    pub(crate) fn image(&self) -> Vec<f64> {
        read((&self.device, &self.queue), &self.hdr)
            .iter()
            .map(|p| (p[0] + p[1] + p[2]) / 3.0)
            .collect()
    }
}
