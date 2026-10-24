use super::*;
use crate::device::GpuContext;
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

pub(crate) fn differing_settings(base: SceneSsao) -> [SceneSsao; 5] {
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
    ]
}

fn settings() -> SceneSsao {
    SceneSsao {
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
                radius_millimeters: radius,
                ..settings()
            })
            .is_err()
        );
    }
    assert!(
        validate_settings(&SceneSsao {
            bias_millimeters: 1000,
            ..settings()
        })
        .is_err()
    );
    assert!(validate_settings(&settings()).is_ok());
}
fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}
fn upload(context: &GpuContext, size: [u32; 2], data: &[f32]) -> wgpu::Texture {
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
fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<f32> {
    let mut readback = Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        1_000_000,
    )
    .unwrap();
    readback
        .read(context.device(), context.submission(), texture)
        .unwrap()
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
        .collect()
}
fn draw(
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
    let ssao = Ssao::new(context.device(), [6, 1], settings()).unwrap();
    let projection = glam::Mat4::IDENTITY;
    let frame = FrameUniform {
        view: projection,
        view_projection: projection,
        camera_position: glam::Vec3::ZERO,
        camera_position_millimeters: [0; 3],
        sun_direction: glam::Vec3::Z,
        sun_irradiance: glam::Vec3::ZERO,
        ambient_sky: glam::Vec3::ONE,
        ambient_ground: glam::Vec3::ONE,
        exposure: 1.0,
        unlit: false,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    ssao.upload(&mut prepare, frame);
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
    renderer.ssao = Some(Ssao::new(context.device(), [5, 3], settings()).unwrap());
    let previous = renderer.ssao.as_ref().unwrap().normals.clone();
    assert!(renderer.resize(context.device(), [0, 7]).is_err());
    assert_eq!(previous, renderer.ssao.as_ref().unwrap().normals);
    assert!(renderer.resize(context.device(), [7, 5]).unwrap());
    assert_eq!(renderer.ssao.as_ref().unwrap().size, [7, 5]);
    assert_ne!(previous, renderer.ssao.as_ref().unwrap().normals);
}
