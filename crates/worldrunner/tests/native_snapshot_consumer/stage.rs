use worldrunner::scene::{gpu::wgpu, post};

pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub struct Factory;

impl post::StageFactory for Factory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &post::FrameTargets,
        _persistent: &post::Resources,
    ) -> Result<post::StageSet, post::PostError> {
        let pipeline = pipeline(device);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("native snapshot caller stage"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(targets.resolved()),
            }],
        });
        Ok(post::StageSet {
            stages: vec![post::Stage::fullscreen(
                "native consumer authored output",
                &pipeline,
                &[&group],
                post::StageTarget::Output,
            )],
            resources: post::Resources::default(),
        })
    }
}

fn pipeline(device: &wgpu::Device) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("native consumer post shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("stage.wgsl").into()),
    });
    let binding = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("native consumer scene input"),
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
        label: Some("native consumer post layout"),
        bind_group_layouts: &[Some(&binding)],
        immediate_size: 0,
    });
    let green = if std::env::var_os("SNAPSHOT_WRONG_STAGE").is_some() {
        0.0
    } else {
        1.0
    };
    post::fullscreen_pipeline(
        device,
        post::FullscreenDescriptor {
            label: Some("native consumer authored post"),
            layout: &layout,
            module: &shader,
            vertex_entry: "vertex",
            fragment_entry: "fragment",
            format: FORMAT,
            constants: &[("MARKER_GREEN", green)],
        },
    )
}
