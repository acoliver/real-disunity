use super::*;
use worldrunner_render_plan::{SceneSsaoFilter, SceneSsaoKernel};

fn metadata(context: &GpuContext, size: [u32; 2], values: &[[u16; 4]]) -> wgpu::Texture {
    let texture = passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    let bytes: Vec<_> = values
        .iter()
        .flatten()
        .flat_map(|word| word.to_le_bytes())
        .collect();
    context.queue().write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(size[0] * 8),
            rows_per_image: Some(size[1]),
        },
        texture.size(),
    );
    texture
}

fn upload_camera(context: &GpuContext, screen: &SceneMetadata) {
    let projection = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 100.0);
    let frame = FrameUniform {
        specular_convention: worldrunner_render_plan::SceneSpecularConvention::default(),
        diffuse_environment: worldrunner_render_plan::SceneDiffuseEnvironment::Hemisphere,
        height_fog: None,
        view: glam::Mat4::IDENTITY,
        view_projection: projection,
        camera_position: glam::Vec3::ZERO,
        camera_position_millimeters: [0; 3],
        sun_direction: glam::Vec3::Z,
        sun_irradiance: glam::Vec3::ZERO,
        ambient_sky: glam::Vec3::ONE,
        ambient_ground: glam::Vec3::ONE,
        exposure: 1.0,
        unlit: false,
        animation_time_milliseconds: 0,
    };
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    screen.upload(&mut prepare, &frame);
    let _uploads = prepare.finish();
    encoder.finish().submit().unwrap();
}

#[test]
fn gpu_spiral_plane_corner_and_normal_orientation() {
    let context = context();
    let size = [33, 33];
    let screen = SceneMetadata::new(
        context.device(),
        size,
        SceneSsao {
            kernel: SceneSsaoKernel::Spiral,
            filter: SceneSsaoFilter::SeparableBilateral,
            ..settings()
        },
    )
    .unwrap();
    upload_camera(&context, &screen);
    let shader = include_str!("spiral.wgsl")
        .replace("LEVEL1_SCALE", "1")
        .replace("LEVEL2_SCALE", "1")
        .replace("LEVEL3_SCALE", "1");
    let pipeline = passes::pipeline(context.device(), &shader);
    let output = passes::texture(context.device(), size, wgpu::TextureFormat::R32Float);
    for corner in [false, true] {
        for normal in [0x3c00, 0xbc00] {
            let values: Vec<_> = (0..size[0] * size[1])
                .map(|index| {
                    let depth = if corner && index % size[0] > 16 {
                        0x3e00
                    } else {
                        0x4000
                    };
                    [0, 0, normal, depth]
                })
                .collect();
            let source = metadata(&context, size, &values);
            let group = passes::group(
                context.device(),
                &pipeline,
                Some(&screen.uniform),
                &[&source, &source, &source, &source],
            );
            draw(&context, &pipeline, &group, &output);
            let centre = read(&context, &output)[16 * 33 + 16];
            if corner && normal == 0x3c00 {
                assert!(centre < 1.0, "corner must occlude: {centre}");
            } else {
                assert_eq!(centre.to_bits(), 1.0f32.to_bits());
            }
        }
    }
}

#[test]
fn gpu_pack_preserves_metre_depth_and_sky_coverage() {
    let context = context();
    let depth = upload(&context, [2, 1], &[2.0, 2000.0]);
    let normals = metadata(&context, [2, 1], &[[0, 0, 0x3c00, 0x3c00], [0; 4]]);
    let output = passes::texture(context.device(), [2, 1], wgpu::TextureFormat::Rgba16Float);
    let pipeline =
        passes::pipeline_format(context.device(), include_str!("pack.wgsl"), output.format());
    let group = passes::group(context.device(), &pipeline, None, &[&depth, &normals]);
    draw(&context, &pipeline, &group, &output);
    let bytes = Readback::new(
        context.device(),
        FrameConfig::new([2, 1], output.format(), 1),
        1024,
    )
    .unwrap()
    .read(context.device(), context.submission(), &output)
    .unwrap();
    let words: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(words, [0, 0, 0x3c00, 0x4000, 0, 0, 0, 0]);
}

#[test]
fn policy_changes_rebuild_resources_and_preserve_existing_pass_slots() {
    use worldrunner_render_post::PassSlot;
    let context = context();
    let old = SceneMetadata::new(context.device(), [5, 3], settings()).unwrap();
    let mut passes = Vec::new();
    let mut views = Vec::new();
    let declaration = old.passes.schedule(&mut passes, &mut views).unwrap();
    assert_eq!(
        declaration
            .passes
            .iter()
            .map(|p| p.slot)
            .collect::<Vec<_>>(),
        [
            PassSlot::DepthPyramid(0),
            PassSlot::DepthPyramid(1),
            PassSlot::DepthPyramid(2),
            PassSlot::DepthPyramid(3),
            PassSlot::Ssao,
            PassSlot::SsaoBlur
        ]
    );
    let next = SceneMetadata::new(
        context.device(),
        [5, 3],
        SceneSsao {
            kernel: SceneSsaoKernel::Spiral,
            filter: SceneSsaoFilter::SeparableBilateral,
            ..settings()
        },
    )
    .unwrap();
    assert_ne!(old.normals, next.normals);
    passes.clear();
    views.clear();
    let declaration = next.passes.schedule(&mut passes, &mut views).unwrap();
    assert!(
        declaration
            .passes
            .iter()
            .any(|p| p.slot == PassSlot::SsaoBlurHorizontal)
    );
    assert!(
        declaration
            .passes
            .iter()
            .any(|p| p.slot == PassSlot::OcclusionMetadata(3))
    );
    for product in [
        crate::frame::metadata::FrameProduct::linear_depth(0),
        crate::frame::metadata::FrameProduct::occlusion_metadata(0),
    ] {
        assert_ne!(
            product.meaning(),
            crate::frame::metadata::MetadataMeaning::DeviceDepth
        );
    }
    assert_ne!(
        crate::frame::metadata::FrameProduct::linear_depth(0).resource(),
        crate::frame::metadata::FrameProduct::occlusion_metadata(0).resource()
    );
}

#[test]
fn allocation_plan_accounts_for_new_kernel_targets() {
    use crate::compositor::RendererConfiguration;
    use worldrunner_render_post::FrameConfig;
    let mut base = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    base.frame = FrameConfig::new([5, 3], wgpu::TextureFormat::Rgba16Float, 1);
    base.hdr = Some((1.0, worldrunner_render_post::ToneMap::Reinhard));
    base.ssao = Some(settings());
    let limits = wgpu::Limits::default();
    let old = base.allocations().validate(&limits).unwrap();
    base.ssao = Some(SceneSsao {
        kernel: SceneSsaoKernel::Spiral,
        filter: SceneSsaoFilter::SeparableBilateral,
        ..settings()
    });
    let next = base.allocations().validate(&limits).unwrap();
    assert_eq!(next - old, (15 + 6 + 2 + 1) * 8 + 15 * 4);
}

#[test]
fn gpu_kernel_reconfiguration_is_transactional_and_can_disable_metadata() {
    use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
    use worldrunner_render_gpu::configuration::AllocationBudget;
    use worldrunner_render_post::FrameConfig;
    let context = context();
    let mut config = RendererConfiguration::legacy([5, 3], wgpu::TextureFormat::Rgba8UnormSrgb);
    config.frame = FrameConfig::new([5, 3], wgpu::TextureFormat::Rgba16Float, 1);
    config.hdr = Some((1.0, worldrunner_render_post::ToneMap::Reinhard));
    config.ssao = Some(settings());
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, config.clone(), AllocationBudget(u64::MAX)).unwrap();
    let initial = renderer.generation();
    config.ssao = Some(SceneSsao {
        kernel: SceneSsaoKernel::Spiral,
        filter: SceneSsaoFilter::SeparableBilateral,
        ..settings()
    });
    renderer.reconfigure(&context, config.clone()).unwrap();
    let changed = renderer.generation();
    assert!(changed > initial);
    config.ssao.as_mut().unwrap().sample_count = 0;
    assert!(renderer.reconfigure(&context, config.clone()).is_err());
    assert_eq!(renderer.generation(), changed);
    config.ssao = None;
    renderer.reconfigure(&context, config).unwrap();
    assert!(renderer.configuration().ssao.is_none());
}

#[test]
fn gpu_linear_depth_is_view_metres_not_device_or_radial_depth() {
    let context = context();
    let screen = SceneMetadata::new(context.device(), [3, 1], settings()).unwrap();
    let projection = glam::Mat4::from_diagonal(glam::Vec4::new(1.0, 1.0, -0.25, 1.0));
    let mut values: Vec<_> = [glam::Mat4::IDENTITY, projection, projection.inverse()]
        .into_iter()
        .flat_map(|matrix| matrix.to_cols_array())
        .collect();
    values.extend([0.0; 8]);
    let bytes: Vec<_> = values.into_iter().flat_map(f32::to_le_bytes).collect();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    let mut prepare = PrepareContext::new(context.device(), &mut encoder).unwrap();
    prepare.upload_buffer(&screen.uniform, 0, &bytes);
    let _uploads = prepare.finish();
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("known device depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &screen
                    .depth
                    .create_view(&wgpu::TextureViewDescriptor::default()),
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.5),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.finish().submit().unwrap();
    let output = passes::texture(context.device(), [3, 1], wgpu::TextureFormat::R32Float);
    let pipeline = passes::pipeline(context.device(), include_str!("linear.wgsl"));
    let group = passes::group(
        context.device(),
        &pipeline,
        Some(&screen.uniform),
        &[&screen.depth],
    );
    draw(&context, &pipeline, &group, &output);
    assert_eq!(
        read(&context, &output)
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        [2.0f32.to_bits(); 3]
    );
}
