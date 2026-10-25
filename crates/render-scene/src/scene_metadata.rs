//! Shared pixel-centre geometry metadata preparation for AO, depth statistics
//! and reflections. Forward MSAA coverage is deliberately a separate domain.
mod geometry;
use crate::{frame::FrameUniform, renderer::ScenePipelines};
pub(crate) use geometry::pipelines;
use worldrunner_render_plan::SceneSsao;
use worldrunner_render_post::{FrameError, PrepareContext};

#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub pipelines: ScenePipelines,
    pub group: wgpu::BindGroup,
    pub ambient: wgpu::BindGroup,
    pub reflection: Option<ScenePipelines>,
}

#[derive(Clone, Debug)]
pub(crate) struct SceneMetadata {
    pub settings: SceneSsao,
    pub size: [u32; 2],
    pub normals: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub geometry: Geometry,
    pub passes: crate::ssao::passes::ScreenPasses,
    pub(crate) uniform: wgpu::Buffer,
}

impl SceneMetadata {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        settings: SceneSsao,
    ) -> Result<Self, FrameError> {
        crate::ssao::validate_settings(&settings)?;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen camera"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (pipelines, group) = geometry::pipelines(device, &uniform, false);
        let (normals, depth) = source_textures(device, size)?;
        let passes = crate::ssao::passes::ScreenPasses::new(
            device, size, &uniform, &depth, &normals, settings,
        );
        let ambient = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filtered ambient occlusion"),
            layout: &crate::ssao::ambient_layout(device),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &passes
                        .filtered
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            }],
        });
        Ok(Self {
            settings,
            size,
            normals,
            depth,
            geometry: Geometry {
                pipelines,
                group,
                ambient,
                reflection: None,
            },
            passes,
            uniform,
        })
    }

    pub fn upload(&self, context: &mut PrepareContext<'_>, frame: FrameUniform) {
        let projection = frame.view_projection * frame.view.inverse();
        let mut data = Vec::new();
        for matrix in [frame.view, projection, projection.inverse()] {
            data.extend(matrix.to_cols_array());
        }
        data.extend([
            crate::ssao::scalar(self.settings.radius_millimeters) / 1000.0,
            crate::ssao::scalar(self.settings.intensity_micro) / 1_000_000.0,
            crate::view::meters_scalar(self.settings.bias_millimeters),
            (crate::view::meters_scalar(self.settings.horizon_angle_millidegrees).to_radians())
                .cos(),
            crate::ssao::scalar(u64::from(self.settings.sample_count)),
            0.0,
            0.0,
            0.0,
        ]);
        context.upload_buffer(
            &self.uniform,
            0,
            &data
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        );
    }
}

fn source_textures(
    device: &wgpu::Device,
    size: [u32; 2],
) -> Result<(wgpu::Texture, wgpu::Texture), FrameError> {
    let normals = crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Rgba16Float);
    let depth = crate::ssao::passes::texture(device, size, wgpu::TextureFormat::Depth32Float);
    crate::frame::metadata::validate_inputs(
        &depth,
        &normals,
        None,
        None,
        crate::frame::metadata::MetadataSampling::PixelCenter,
    )
    .map_err(|_| FrameError::SsaoSettings("source metadata contract"))?;
    Ok((normals, depth))
}

#[cfg(test)]
thread_local! {
    pub(crate) static EXECUTED_PASSES: std::cell::RefCell<Vec<worldrunner_render_post::PassSlot>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
pub(crate) fn record_pass(slot: worldrunner_render_post::PassSlot) {
    EXECUTED_PASSES.with_borrow_mut(|passes| passes.push(slot));
}
