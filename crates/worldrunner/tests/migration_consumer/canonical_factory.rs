use super::{GpuContext, post, wgpu};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use worldrunner::scene::gpu::{
    SceneView, draw::SceneGpu, dynamic::DynamicQuad, frame::FrameUniform,
    offscreen::OffscreenTarget, renderer::ScenePipelines,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Factory {
    builds: Arc<AtomicUsize>,
    marker_green: f64,
}
impl post::StageFactory for Factory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &post::FrameTargets,
        _persistent: &post::Resources,
    ) -> Result<post::StageSet, post::PostError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        let pipeline = pipeline(device, self.marker_green);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("migration authored stage input"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(targets.resolved()),
            }],
        });
        Ok(post::StageSet {
            stages: vec![post::Stage::fullscreen(
                "consumer overlay and output",
                &pipeline,
                &[&group],
                post::StageTarget::Output,
            )],
            resources: post::Resources::default(),
        })
    }
}

fn pipeline(device: &wgpu::Device, marker_green: f64) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("migration authored post"),
        source: wgpu::ShaderSource::Wgsl(include_str!("canonical_post.wgsl").into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("migration scene texture"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("migration caller layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    post::fullscreen_pipeline(
        device,
        post::FullscreenDescriptor {
            label: Some("migration post"),
            layout: &layout,
            module: &shader,
            vertex_entry: "vertex",
            fragment_entry: "fragment",
            format: FORMAT,
            constants: &[("MARKER_GREEN", marker_green)],
        },
    )
}

pub struct Fixture {
    chain: post::PostChain<Factory>,
    pipelines: ScenePipelines,
    scene: SceneGpu,
}

impl Fixture {
    pub fn new(context: &GpuContext) -> Self {
        Self::with_marker(context, 1.0)
    }

    pub fn with_marker(context: &GpuContext, marker_green: f64) -> Self {
        let chain = post::PostChain::new(
            context.device(),
            post::FrameConfig::new([65, 47], FORMAT, 1),
            post::Resources::default(),
            Factory {
                builds: Arc::default(),
                marker_green,
            },
        )
        .unwrap();
        let pipelines = ScenePipelines::new(context.device(), FORMAT);
        let mut scene = SceneGpu::empty(context.device(), context.queue(), &pipelines);
        scene
            .set_dynamic_quads(
                context.device(),
                context.queue(),
                &pipelines,
                &[DynamicQuad {
                    tint: [1., 0., 0., 1.],
                    ..DynamicQuad::default()
                }],
            )
            .unwrap();
        Self {
            chain,
            pipelines,
            scene,
        }
    }

    pub fn builds(&self) -> usize {
        self.chain.factory().builds.load(Ordering::SeqCst)
    }

    pub fn render(&mut self, context: &GpuContext, size: [u32; 2]) -> Vec<u8> {
        self.chain.resize(context.device(), size).unwrap();
        let resolved = super::super::super::camera::planar(
            size,
            worldrunner::scene::camera::ViewHandedness::RightHanded,
        );
        let view = SceneView::from_resolved(resolved);
        self.scene
            .set_frame(context.queue(), FrameUniform::unlit(&view));
        let depth = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
        let output =
            post::color_texture(context.device(), "consumer-owned post output", size, FORMAT);
        let mut encoder = context
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        draw(
            &mut encoder,
            self.chain.targets().scene(),
            depth.depth_view(),
            &self.scene,
            &self.pipelines,
        );
        self.chain.encode(
            &mut encoder,
            &output.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        let mut readback = post::Readback::new(
            context.device(),
            post::FrameConfig::new(size, FORMAT, 1),
            1_048_576,
        )
        .unwrap();
        readback.encode(&mut encoder, &output).unwrap();
        let submission = context.queue().submit([encoder.finish()]);
        readback
            .read_encoded(context.device(), Some(submission))
            .unwrap()
    }
}

fn draw(
    encoder: &mut wgpu::CommandEncoder,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    scene: &SceneGpu,
    pipelines: &ScenePipelines,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("consumer scene before post"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    scene.draw(&mut pass, pipelines);
}
