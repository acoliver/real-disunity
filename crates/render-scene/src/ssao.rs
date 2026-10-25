//! Ambient occlusion settings, sampling and reconstruction.
pub(crate) mod bilateral;
pub(crate) mod passes;
#[cfg(test)]
pub(crate) mod tests;

use worldrunner_render_plan::SceneSsao;
use worldrunner_render_post::FrameError;

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

pub(crate) fn scalar(value: u64) -> f32 {
    // Settings validation bounds every conversion to an exactly representable integer.
    let value = u32::try_from(value).expect("bounded SSAO setting");
    f32::from(u16::try_from(value >> 16).expect("upper half")) * 65536.0
        + f32::from(u16::try_from(value & 65535).expect("lower half"))
}

fn replace_marker(source: &str, marker: &str, replacement: &str) -> String {
    assert_eq!(source.matches(marker).count(), 1, "shader marker {marker}");
    source.replacen(marker, replacement, 1)
}
