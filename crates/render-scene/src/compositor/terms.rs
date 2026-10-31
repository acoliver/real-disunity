//! Request-owned pixel-centre opaque replay products, before exposure and fog.
use crate::{draw::SceneGpu, renderer::ScenePipelines};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureTerm {
    /// Linear reflectance after textures, terrain blending and projected decals.
    Albedo,
    /// Strength-adjusted directional visibility in [0, 1], replicated in RGB.
    SunVisibility,
}

/// Storage is owned by one capture request and must be included in its budget.
pub struct TermCapture {
    pub texture: wgpu::Texture,
    depth: wgpu::Texture,
    term: CaptureTerm,
}
impl TermCapture {
    pub fn new(device: &wgpu::Device, size: [u32; 2], term: CaptureTerm) -> Self {
        Self {
            texture: crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Rgba16Float),
            depth: crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Depth32Float),
            term,
        }
    }
    pub fn allocations(size: [u32; 2]) -> worldrunner_render_gpu::configuration::AllocationSet {
        let mut set = worldrunner_render_gpu::configuration::AllocationSet::default();
        for format in [
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureFormat::Depth32Float,
        ] {
            set.texture(
                "opaque term replay",
                [size[0], size[1], 1],
                format,
                [1, 1],
                1,
            );
        }
        set
    }
    pub(crate) fn textures(&self) -> [&wgpu::Texture; 2] {
        [&self.texture, &self.depth]
    }
    /// Replay the current drawing frame. No clock, upload, residency or history advancement.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &SceneGpu,
    ) {
        self.encode_with_shadow(device, encoder, gpu, None);
    }
    pub(crate) fn encode_with_shadow(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        gpu: &SceneGpu,
        shadow: Option<(&wgpu::BindGroupLayout, &wgpu::BindGroup)>,
    ) {
        let pipelines = pipelines(device, self.term, shadow.map(|s| s.0));
        let color = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let depth = self
            .depth
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("opaque term replay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        if let Some((_, group)) = shadow {
            pass.set_bind_group(2, group, &[]);
        }
        gpu.draw_opaque_capture(&mut pass, &pipelines, shadow.map(|s| s.1));
    }
}
fn pipelines(
    device: &wgpu::Device,
    term: CaptureTerm,
    shadow: Option<&wgpu::BindGroupLayout>,
) -> ScenePipelines {
    let (object, terrain) = crate::shadows::capture_sources(shadow.is_some());
    let (mesh_value, terrain_value) = match term {
        CaptureTerm::Albedo => ("albedo_sample.rgb", "projected.albedo"),
        CaptureTerm::SunVisibility => (
            "vec3<f32>(sunlight_visibility(input.world_position, normal))",
            "vec3<f32>(sunlight_visibility(input.world_position, projected.normal))",
        ),
    };
    let object = object.replace(
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
        &format!("return vec4<f32>({mesh_value}, albedo_sample.a);"),
    );
    let terrain = terrain.replace(
        "return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), 1.0);",
        &format!("return vec4<f32>({terrain_value}, 1.0);"),
    );
    ScenePipelines::with_environment(
        device,
        (wgpu::TextureFormat::Rgba16Float, 1),
        &object,
        &terrain,
        shadow,
    )
}
