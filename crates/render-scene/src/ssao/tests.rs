use super::*;
use crate::device::GpuContext;
use crate::{frame::FrameUniform, scene_metadata::SceneMetadata};
use worldrunner_render_post::PrepareContext;
use worldrunner_render_post::{FrameConfig, Readback};

#[test]
fn ambient_injection_uses_one_explicit_marker_independent_of_expression_spacing() {
    for source in [crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL] {
        let respaced = source.replace("* ambient_occlusion", "*\n ambient_occlusion");
        let result = ambient_source(&respaced);
        assert_eq!(result.matches("* screen_ao(world_position)").count(), 1);
        assert!(!result.contains("/* SCREEN_AMBIENT_FACTOR */"));
    }
}

#[test]
#[should_panic(expected = "shader marker")]
fn missing_shader_marker_fails() {
    ambient_source("fn unrelated() {}");
}

#[test]
#[should_panic(expected = "shader marker")]
fn duplicate_shader_marker_fails() {
    ambient_source("/* SCREEN_AMBIENT_FACTOR */ /* SCREEN_AMBIENT_FACTOR */");
}

pub(crate) fn differing_settings(base: SceneSsao) -> [SceneSsao; 7] {
    [
        SceneSsao {
            radius_millimeters: base.radius_millimeters + 1,
            ..base
        },
        SceneSsao {
            intensity_micro: 0,
            ..base
        },
        SceneSsao {
            sample_count: base.sample_count - 1,
            ..base
        },
        SceneSsao {
            bias_millimeters: base.bias_millimeters + 1,
            ..base
        },
        SceneSsao {
            horizon_angle_millidegrees: base.horizon_angle_millidegrees - 1,
            ..base
        },
        SceneSsao {
            kernel: worldrunner_render_plan::SceneSsaoKernel::Spiral,
            ..base
        },
        SceneSsao {
            filter: worldrunner_render_plan::SceneSsaoFilter::SeparableBilateral,
            ..base
        },
    ]
}

pub(super) fn settings() -> SceneSsao {
    SceneSsao {
        kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
        filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
        radius_millimeters: 1000,
        intensity_micro: 1_000_000,
        sample_count: 32,
        bias_millimeters: 10,
        horizon_angle_millidegrees: 90_000,
    }
}
#[test]
fn settings_fail_before_allocation() {
    for sample_count in [0, 65, u32::MAX] {
        assert!(matches!(
            validate_settings(&SceneSsao {
                sample_count,
                ..settings()
            }),
            Err(FrameError::SsaoSettings(_))
        ));
    }
    for radius in [0, 1_000_001, u64::MAX] {
        assert!(
            validate_settings(&SceneSsao {
                kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
                filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
                radius_millimeters: radius,
                ..settings()
            })
            .is_err()
        );
    }
    assert!(
        validate_settings(&SceneSsao {
            kernel: worldrunner_render_plan::SceneSsaoKernel::Hemisphere,
            filter: worldrunner_render_plan::SceneSsaoFilter::DepthAwareBox,
            bias_millimeters: 1000,
            ..settings()
        })
        .is_err()
    );
    assert!(validate_settings(&settings()).is_ok());
}
pub(super) fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}
pub(super) fn upload(context: &GpuContext, size: [u32; 2], data: &[f32]) -> wgpu::Texture {
    let texture = passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
    let bytes: Vec<_> = data.iter().flat_map(|value| value.to_le_bytes()).collect();
    context.queue().write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 4),
            rows_per_image: Some(size[1]),
        },
        texture.size(),
    );
    texture
}
pub(super) fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<f32> {
    let mut readback = Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        1_000_000,
    )
    .unwrap();
    let bytes = readback
        .read(context.device(), context.submission(), texture)
        .unwrap();
    if texture.format() == wgpu::TextureFormat::Rg16Float {
        bytes
            .chunks_exact(2)
            .map(|word| decode_half(u16::from_le_bytes(word.try_into().unwrap())))
            .collect()
    } else {
        bytes
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect()
    }
}
pub(super) fn draw(
    context: &GpuContext,
    pipeline: &wgpu::RenderPipeline,
    group: &wgpu::BindGroup,
    output: &wgpu::Texture,
) {
    let mut encoder = context.submission().begin(context.device()).unwrap();
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("test screen pass"),
        wgpu::RenderPassColorAttachment {
            view: &output.create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        },
        pipeline,
        &[group],
    );
    encoder.finish().submit().unwrap();
}
#[test]
fn gpu_odd_extent_pyramid_includes_last_row_and_column() {
    let context = context();
    for size in [[5, 3], [1, 7], [7, 1]] {
        let mut values = vec![9.0; usize::try_from(size[0] * size[1]).unwrap()];
        *values.last_mut().unwrap() = 0.25;
        let mut source = upload(&context, size, &values);
        let pipeline = passes::pipeline(context.device(), include_str!("reduce.wgsl"));
        let mut extent = size;
        while extent != [1, 1] {
            extent = [extent[0].div_ceil(2), extent[1].div_ceil(2)];
            let output = passes::texture(context.device(), extent, wgpu::TextureFormat::R32Float);
            let group = passes::group(context.device(), &pipeline, None, &[&source]);
            draw(&context, &pipeline, &group, &output);
            source = output;
        }
        assert_eq!(read(&context, &source), [0.25]);
    }
}
#[test]
fn gpu_bilateral_blur_preserves_depth_edges() {
    let context = context();
    let ssao = SceneMetadata::new(context.device(), [6, 1], settings()).unwrap();
    let projection = glam::Mat4::IDENTITY;
    let frame = FrameUniform {
        specular_convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        diffuse_environment: worldrunner_render_plan::SceneDiffuseEnvironment::Hemisphere,
        height_fog: None,
        view: projection,
        view_projection: projection,
        camera_position: glam::Vec3::ZERO,
        camera_position_millimeters: [0; 3],
        sun_direction: glam::Vec3::Z,
        sun_ao_intensity: None,
        sun_irradiance: glam::Vec3::ZERO,
        ambient_sky: glam::Vec3::ONE,
        ambient_ground: glam::Vec3::ONE,
        exposure: 1.0,
        unlit: false,
        animation_time_milliseconds: 0,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    ssao.upload(&mut prepare, &frame);
    let _uploads = prepare.finish();
    encoder.finish().submit().unwrap();
    let depth = upload(&context, [6, 1], &[1.0, 1.0, 1.0, 3.0, 3.0, 3.0]);
    let ao = upload(&context, [6, 1], &[0.2, 0.2, 0.2, 1.0, 1.0, 1.0]);
    let output = passes::texture(context.device(), [6, 1], wgpu::TextureFormat::R32Float);
    let pipeline = passes::pipeline(context.device(), include_str!("blur.wgsl"));
    let group = passes::group(
        context.device(),
        &pipeline,
        Some(&ssao.uniform),
        &[&depth, &ao],
    );
    draw(&context, &pipeline, &group, &output);
    for (actual, expected) in read(&context, &output)
        .iter()
        .zip([0.2, 0.2, 0.2, 1.0, 1.0, 1.0])
    {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn hdr_resize_rebuilds_screen_targets_and_preserves_them_on_error() {
    use crate::compositor::FrameRenderer;
    use worldrunner_render_post::ToneMap;
    let context = context();
    let mut renderer = FrameRenderer::hdr(
        &context,
        [5, 3],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        4,
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    renderer.ssao = Some(SceneMetadata::new(context.device(), [5, 3], settings()).unwrap());
    let previous = renderer.ssao.as_ref().unwrap().normals.clone();
    assert!(renderer.resize(context.device(), [0, 7]).is_err());
    assert_eq!(previous, renderer.ssao.as_ref().unwrap().normals);
    assert!(renderer.resize(context.device(), [7, 5]).unwrap());
    assert_eq!(renderer.ssao.as_ref().unwrap().size, [7, 5]);
    assert_ne!(previous, renderer.ssao.as_ref().unwrap().normals);
}

#[test]
fn gpu_average_metadata_is_not_conservative_depth() {
    let context = context();
    let source = passes::texture(context.device(), [3, 1], wgpu::TextureFormat::Rgba16Float);
    let words: [u16; 12] = [
        0, 0, 0x3c00, 0x4000, 0, 0, 0x3c00, 0x4600, 0, 0, 0x3c00, 0x4800,
    ];
    context.queue().write_texture(
        source.as_image_copy(),
        &words
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(24),
            rows_per_image: Some(1),
        },
        source.size(),
    );
    let pipeline = passes::pipeline_format(
        context.device(),
        include_str!("average.wgsl"),
        wgpu::TextureFormat::Rgba16Float,
    );
    let output = passes::texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba16Float);
    let group = passes::group(context.device(), &pipeline, None, &[&source]);
    draw(&context, &pipeline, &group, &output);
    let bytes = Readback::new(
        context.device(),
        FrameConfig::new([2, 1], output.format(), 1),
        1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), &output)
    .unwrap();
    let actual: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(actual, [0, 0, 0x3c00, 0x4400, 0, 0, 0x3c00, 0x4800]);
}

#[test]
fn gpu_bilateral_axes_have_exact_impulse_and_reject_depth_steps() {
    let context = context();
    for (size, axis) in [([5, 1], [1, 0]), ([1, 5], [0, 1])] {
        let depths = upload(&context, size, &[1.0, 1.0, 1.0, 4.0, 4.0]);
        let input = upload(&context, size, &[0.0, 0.0, 1.0, 1.0, 1.0]);
        let output = passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
        let shader = super::bilateral::source(axis);
        let pipeline = passes::pipeline(context.device(), &shader);
        let group = passes::group(context.device(), &pipeline, None, &[&depths, &input]);
        draw(&context, &pipeline, &group, &output);
        let values = read(&context, &output);
        assert_eq!(values[3..], [1.0, 1.0]);
        assert!(values[0] > 0.0 && values[0] < 1.0);
        assert!(values[2] > values[0]);
        assert_eq!(values[1].to_bits(), 0.0f32.to_bits());
    }
}

#[path = "numerical.rs"]
mod numerical;

pub(crate) fn decode_half(word: u16) -> f32 {
    let exponent = i32::from((word >> 10) & 31);
    let fraction = f32::from(word & 1023);
    let value = if exponent == 0 {
        fraction * 2.0f32.powi(-24)
    } else {
        (1.0 + fraction / 1024.0) * 2.0f32.powi(exponent - 15)
    };
    if word & 32768 == 0 { value } else { -value }
}
