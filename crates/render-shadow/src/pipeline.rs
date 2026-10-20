/// Optional alpha-test fragment stage. Colour targets are deliberately not configurable.
#[derive(Clone, Debug)]
pub struct DepthFragmentState<'a> {
    pub module: &'a wgpu::ShaderModule,
    pub entry_point: Option<&'a str>,
    pub compilation_options: wgpu::PipelineCompilationOptions<'a>,
}

/// Shader and geometry policy supplied by a renderer; fixed depth GPU mechanics live here.
#[derive(Clone, Debug)]
pub struct DepthPipelineDescriptor<'a> {
    pub label: Option<&'a str>,
    pub layout: &'a wgpu::PipelineLayout,
    pub vertex: wgpu::VertexState<'a>,
    pub primitive: wgpu::PrimitiveState,
    pub fragment: Option<DepthFragmentState<'a>>,
    pub compare: wgpu::CompareFunction,
    pub bias: wgpu::DepthBiasState,
}

/// Builds a single-sample, depth-writing pipeline matching [`crate::DepthMap`].
pub fn create_depth_pipeline(
    device: &wgpu::Device,
    descriptor: DepthPipelineDescriptor<'_>,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: descriptor.label,
        layout: Some(descriptor.layout),
        vertex: descriptor.vertex,
        primitive: descriptor.primitive,
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(descriptor.compare),
            stencil: wgpu::StencilState::default(),
            bias: descriptor.bias,
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: descriptor.fragment.map(|fragment| wgpu::FragmentState {
            module: fragment.module,
            entry_point: fragment.entry_point,
            compilation_options: fragment.compilation_options,
            targets: &[],
        }),
        multiview_mask: None,
        cache: None,
    })
}
