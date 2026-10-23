//! Full-resolution single-sample metadata, independent of forward MSAA coverage.
pub(crate) mod passes;
#[cfg(test)]
pub(crate) mod tests;

use crate::{frame::FrameUniform, renderer::ScenePipelines};
use worldrunner_render_plan::SceneSsao;
use worldrunner_render_post::{FrameError, PrepareContext};

pub(crate) fn validate_settings(settings: &SceneSsao) -> Result<(), FrameError> {
    if !(1..=64).contains(&settings.sample_count) {
        return Err(FrameError::SsaoSettings("sample count must be 1..=64"));
    }
    if settings.radius_millimeters == 0 || settings.radius_millimeters > 1_000_000 {
        return Err(FrameError::SsaoSettings(
            "radius must be 1..=1000000 millimetres",
        ));
    }
    if u64::from(settings.bias_millimeters) >= settings.radius_millimeters {
        return Err(FrameError::SsaoSettings("bias must be smaller than radius"));
    }
    if settings.intensity_micro > 2_000_000 || settings.horizon_angle_millidegrees > 180_000 {
        return Err(FrameError::SsaoSettings(
            "intensity or horizon angle out of range",
        ));
    }
    Ok(())
}

pub(crate) fn ambient_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ambient occlusion"),
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
    })
}

pub(crate) fn ambient_source(source: &str) -> String {
    format!(
        "{}\n{}",
        replace_marker(
            source,
            "/* SCREEN_AMBIENT_FACTOR */",
            "* screen_ao(world_position)"
        ),
        r"
@group(3) @binding(0) var ambient_texture: texture_2d<f32>;
fn screen_ao(world: vec3<f32>) -> f32 {
    let clip = frame.view_projection * vec4<f32>(world, 1.0);
    let uv = clip.xy / clip.w * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let size = vec2<i32>(textureDimensions(ambient_texture));
    return textureLoad(ambient_texture, clamp(vec2<i32>(uv * vec2<f32>(size)), vec2<i32>(0), size - vec2<i32>(1)), 0).r;
}"
    )
}

#[derive(Clone, Debug)]
pub(crate) struct Geometry {
    pub pipelines: ScenePipelines,
    pub group: wgpu::BindGroup,
    pub ambient: wgpu::BindGroup,
}

#[derive(Debug)]
pub(crate) struct Ssao {
    pub settings: SceneSsao,
    pub size: [u32; 2],
    pub normals: wgpu::Texture,
    pub depth: wgpu::Texture,
    pub geometry: Geometry,
    pub passes: passes::ScreenPasses,
    uniform: wgpu::Buffer,
}

impl Ssao {
    pub fn new(
        device: &wgpu::Device,
        size: [u32; 2],
        settings: SceneSsao,
    ) -> Result<Self, FrameError> {
        validate_settings(&settings)?;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen camera"),
            size: 224,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("prepass camera"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("prepass camera"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let pipelines = prepass_pipelines(device, &layout);
        let normals = passes::texture(device, size, wgpu::TextureFormat::Rgba16Float);
        let depth = passes::texture(device, size, wgpu::TextureFormat::Depth32Float);
        let passes = passes::ScreenPasses::new(device, size, &uniform, &depth, &normals);
        let ambient = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("filtered ambient occlusion"),
            layout: &ambient_layout(device),
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
            scalar(self.settings.radius_millimeters) / 1000.0,
            scalar(self.settings.intensity_micro) / 1_000_000.0,
            crate::view::meters_scalar(self.settings.bias_millimeters),
            (crate::view::meters_scalar(self.settings.horizon_angle_millidegrees).to_radians())
                .cos(),
            scalar(u64::from(self.settings.sample_count)),
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

fn scalar(value: u64) -> f32 {
    // Settings validation bounds every conversion to an exactly representable integer.
    let value = u32::try_from(value).expect("bounded SSAO setting");
    f32::from(u16::try_from(value >> 16).expect("upper half")) * 65536.0
        + f32::from(u16::try_from(value & 65535).expect("lower half"))
}

fn replace_marker(source: &str, marker: &str, replacement: &str) -> String {
    assert_eq!(source.matches(marker).count(), 1, "shader marker {marker}");
    source.replacen(marker, replacement, 1)
}

fn prepass_pipelines(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> ScenePipelines {
    let prefix = include_str!("ssao/camera.wgsl");
    let objects = format!(
        "{}\n{prefix}\n{}",
        replace_marker(
            &replace_marker(
                crate::shader::ENVIRONMENT_WGSL,
                "fs_opaque",
                "unused_opaque"
            ),
            "fs_masked",
            "unused_masked",
        ),
        include_str!("ssao/prepass.wgsl")
    );
    let terrain = format!(
        "{}\n{prefix}\n{}",
        replace_marker(crate::shader::TERRAIN_WGSL, "fs_terrain", "unused_terrain"),
        r"
@fragment fn fs_terrain(input: TerrainVertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(normalize((screen.view * vec4<f32>(input.world_normal, 0.0)).xyz), 1.0);
}"
    );
    ScenePipelines::with_screen_space(
        device,
        (wgpu::TextureFormat::Rgba16Float, 1),
        &objects,
        &terrain,
        None,
        Some(layout),
    )
}
