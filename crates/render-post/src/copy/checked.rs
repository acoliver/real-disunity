use crate::{
    CopyOutput,
    contracts::{
        CheckedBindings, CheckedDraw, CheckedView, ContractError, ResourceRegistry, Semantic,
        TextureBinding, TextureBindingKind,
    },
};

impl CopyOutput {
    /// Stock nearest-copy draw packet for an executor-owned mixed pass.
    /// Accepts D2, single-sample float sources with a stock-supported renderable format.
    /// The caller must declare the source as a sampled read in that pass.
    pub fn checked_draw(
        device: &wgpu::Device,
        registry: &ResourceRegistry,
        source: CheckedView,
        samples: u32,
    ) -> Result<CheckedDraw, ContractError> {
        let format = validate_source(device, registry, &source, samples)?;
        let bindings = CheckedBindings::build(
            device,
            registry,
            &[TextureBinding {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                view: source,
                samples: 1,
                semantic: Semantic::LinearColor,
                kind: TextureBindingKind::Sampled(wgpu::TextureSampleType::Float {
                    filterable: false,
                }),
            }],
        )?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("checked stock copy"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../copy.wgsl").into()),
        });
        CheckedDraw::new(
            device,
            &wgpu::RenderPipelineDescriptor {
                label: Some("checked stock copy"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vertex"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fragment"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            },
            vec![bindings],
            0..3,
            0..1,
        )
    }
}

fn validate_source(
    device: &wgpu::Device,
    registry: &ResourceRegistry,
    source: &CheckedView,
    samples: u32,
) -> Result<wgpu::TextureFormat, ContractError> {
    registry.check_view(source)?;
    let texture = source.view().texture();
    let format = texture.format();
    if source.dimension() != wgpu::TextureViewDimension::D2
        || texture.sample_count() != 1
        || super::validate_output(device, format).is_err()
        || !format
            .guaranteed_format_features(device.features())
            .flags
            .sample_count_supported(samples)
    {
        return Err(ContractError::CopyOutput(source.allocation()));
    }
    Ok(format)
}
