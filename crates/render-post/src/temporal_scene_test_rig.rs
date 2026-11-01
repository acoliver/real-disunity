//! Test rig of the temporal stage on lavapipe for analytic scenes: a WGSL scene shader that
//! writes colour, backward motion and device depth at the internal extent is rasterised for
//! each frame of the jitter sequence, the temporal chain resolves it, and the HDR output is read
//! back. The scene shader reads a uniform of the internal extent, the jitter in internal pixels
//! and four scene values (`tail`).
use crate::hdr_temporal_build_tests::{public_factory, settings};
use crate::taa_gpu_test_support::read;
use crate::temporal_quality_tests::Recording;
use crate::{
    FrameConfig, HdrPostSettings, PostChain, ProjectionJitter, Resources, TaaSettings,
    TemporalBlock, TemporalCamera,
};
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

pub(crate) const OUTPUT: [u32; 2] = [160, 96];

pub(crate) struct SceneRig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    chain: PostChain<Recording>,
    hdr: wgpu::Texture,
    history: wgpu::Texture,
    sink: wgpu::Texture,
    pipeline: wgpu::RenderPipeline,
    mask_pipeline: Option<wgpu::RenderPipeline>,
    render: [u32; 2],
    output: [u32; 2],
    slot_output: Arc<Mutex<Option<wgpu::Texture>>>,
    slot_history: Arc<Mutex<Option<wgpu::Texture>>>,
    slot_lock_bank: Arc<Mutex<Option<wgpu::Texture>>>,
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

/// The producer of the mask: the scene module's `paint_mask` entry, drawn with the maximum blend
/// the mask contract asks of producers.
fn mask_pipeline(device: &wgpu::Device, source: &str) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("analytic mask producer"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let max = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("analytic mask producer"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertex"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("paint_mask"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: crate::TEMPORAL_MASK_FORMAT,
                blend: Some(wgpu::BlendState {
                    color: max,
                    alpha: max,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
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
        Self::with_output(gpu, render, OUTPUT, scene_wgsl)
    }

    /// A chain at the given render and output extents.
    pub(crate) fn with_output(
        gpu: &(wgpu::Device, wgpu::Queue),
        render: [u32; 2],
        output: [u32; 2],
        scene_wgsl: String,
    ) -> Self {
        Self::with_settings(gpu, [render, output], scene_wgsl, &settings())
    }

    /// A chain built from other post settings, for example another display exposure.
    pub(crate) fn with_settings(
        gpu: &(wgpu::Device, wgpu::Queue),
        [render, output]: [[u32; 2]; 2],
        scene_wgsl: String,
        post: &HdrPostSettings,
    ) -> Self {
        let (device, queue) = gpu.clone();
        let config = FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1)
            .with_render_extent(render);
        let slot_output = Arc::new(Mutex::new(None));
        let slot_history = Arc::new(Mutex::new(None));
        let slot_lock_bank = Arc::new(Mutex::new(None));
        let factory = Recording {
            inner: public_factory(&device, post),
            output: slot_output.clone(),
            history: slot_history.clone(),
            lock_bank: slot_lock_bank.clone(),
        };
        let chain = PostChain::new(&device, config, Resources::default(), factory).unwrap();
        let hdr = slot_output
            .lock()
            .unwrap()
            .take()
            .expect("slot 0 was built");
        let history = slot_history
            .lock()
            .unwrap()
            .take()
            .expect("the history was built");
        let sink =
            crate::color_texture(&device, "sink", output, wgpu::TextureFormat::Rgba8UnormSrgb);
        Self {
            mask_pipeline: scene_wgsl
                .contains("fn paint_mask")
                .then(|| mask_pipeline(&device, &scene_wgsl)),
            pipeline: pipeline(&device, scene_wgsl),
            device,
            queue,
            chain,
            hdr,
            history,
            sink,
            render,
            output,
            slot_output,
            slot_history,
            slot_lock_bank,
            jitter: ProjectionJitter::for_extents(render, output).unwrap(),
        }
    }

    /// Replaces both extents of the chain; the next frame must reset the history.
    pub(crate) fn resize(&mut self, render: [u32; 2], output: [u32; 2]) {
        self.chain
            .resize_extents(&self.device, render, output)
            .unwrap();
        self.hdr = self
            .slot_output
            .lock()
            .unwrap()
            .take()
            .expect("slot 0 was rebuilt");
        self.history = self
            .slot_history
            .lock()
            .unwrap()
            .take()
            .expect("the history was rebuilt");
        self.sink = crate::color_texture(
            &self.device,
            "sink",
            output,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        self.render = render;
        self.output = output;
        self.jitter = ProjectionJitter::for_extents(render, output).unwrap();
    }

    /// The group of the scene uniform for `pipeline`.
    fn frame_group(
        &self,
        pipeline: &wgpu::RenderPipeline,
        offset: [f32; 2],
        tail: [f32; 4],
    ) -> wgpu::BindGroup {
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
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        })
    }

    /// Draws the scene's mask producer into the frame mask, when the scene has one.
    fn paint_mask(&self, encoder: &mut wgpu::CommandEncoder, offset: [f32; 2], tail: [f32; 4]) {
        let (Some(pipeline), Some(view)) =
            (&self.mask_pipeline, self.chain.targets().temporal_mask())
        else {
            return;
        };
        let group = self.frame_group(pipeline, offset, tail);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("analytic mask producer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Rasterises the scene into colour, velocity and depth at the internal extent, then the mask.
    fn paint(&self, encoder: &mut wgpu::CommandEncoder, offset: [f32; 2], tail: [f32; 4]) {
        let group = self.frame_group(&self.pipeline, offset, tail);
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
        drop(pass);
        self.paint_mask(encoder, offset, tail);
    }

    /// Frame `index` of the jitter sequence with the scene values `tail`. The camera term is
    /// uploaded when given, otherwise the slot keeps the zeroed block of a stage without one.
    pub(crate) fn frame(
        &self,
        index: u64,
        history_valid: bool,
        (tail, camera): ([f32; 4], Option<TemporalCamera>),
    ) {
        let commands = self.record(index, history_valid, tail, camera);
        self.queue.submit([commands]);
    }

    /// A frame of the object bar scene recorded and dropped without submission.
    pub(crate) fn cancelled_frame(&self, index: u64, history_valid: bool, tail: [f32; 4]) {
        drop(self.record(
            index,
            history_valid,
            tail,
            Some(crate::taa_reconstruct_test_rig::static_camera()),
        ));
    }

    /// A producer of a frame that was later cancelled: the mask write is submitted, the rest of
    /// the frame never is.
    pub(crate) fn abandon_mask_write(&self, value: f64) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        crate::temporal_mask_clear_tests::write_mask(&mut encoder, self.chain.targets(), value);
        self.queue.submit([encoder.finish()]);
    }

    /// The mask bytes after the last submitted work, row major.
    pub(crate) fn mask(&self) -> Vec<[u8; 2]> {
        let texture = self.chain.targets().temporal_mask_texture().unwrap();
        crate::temporal_mask_clear_tests::read_mask((&self.device, &self.queue), texture)
    }

    fn record(
        &self,
        index: u64,
        history_valid: bool,
        tail: [f32; 4],
        camera: Option<TemporalCamera>,
    ) -> wgpu::CommandBuffer {
        let offset = self.jitter.offset_pixels(index);
        let block = TemporalBlock {
            render: self.render,
            output: self.output,
            jitter: offset,
            phase_count: self.jitter.length(),
            settings: TaaSettings::new(0.9),
            history_valid,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.chain.targets().encode_mask_clear(&mut encoder);
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
        encoder.finish()
    }

    /// The HDR reconstruction of the last frame, mean of the colour channels, row major.
    pub(crate) fn image(&self) -> Vec<f64> {
        read((&self.device, &self.queue), &self.hdr)
            .iter()
            .map(|p| (p[0] + p[1] + p[2]) / 3.0)
            .collect()
    }
}

impl SceneRig {
    /// The shared history texture after the last frame: the pre-tone reconstruction, row major.
    pub(crate) fn history_image(&self) -> Vec<[f64; 4]> {
        read((&self.device, &self.queue), &self.history)
    }

    /// The lock bank of slot 0 after the last frame, row major; `None` with the locks off.
    pub(crate) fn lock_bank(&self) -> Option<Vec<[f64; 4]>> {
        let bank = self.slot_lock_bank.lock().unwrap().clone()?;
        Some(read((&self.device, &self.queue), &bank))
    }

    /// The tone mapped bytes the chain wrote to its sink in the last frame, RGBA, row major.
    pub(crate) fn display_bytes(&self) -> Vec<u8> {
        crate::Readback::new(
            &self.device,
            FrameConfig::new(self.output, wgpu::TextureFormat::Rgba8UnormSrgb, 1),
            1_000_000,
        )
        .unwrap()
        .read(
            &self.device,
            &crate::test_authority(&self.queue),
            &self.sink,
        )
        .unwrap()
    }
}
