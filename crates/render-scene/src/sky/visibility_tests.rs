use super::scene_tests::*;
use crate::{device::GpuContext, draw::SceneGpu, frame::FrameUniform, renderer::ScenePipelines};
use worldrunner_render_post::{AttachmentView, ResourceRef};

#[test]
fn celestial_visibility_composites_corresponding_msaa_samples() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        if samples > 1 {
            assert_celestial_composites_samples(&context, samples);
        }
    }
}

fn assert_celestial_composites_samples(context: &GpuContext, samples: u32) {
    let source = scene(0, false, false);
    let image = capture_celestial(context, &source, samples);
    assert_eq!(
        image.rgb(4).map(f32::to_bits),
        [0.0_f32, 1.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(
        image.rgb(0).map(f32::to_bits),
        [0.0_f32, 0.0, 2.0].map(f32::to_bits)
    );
}

fn capture_celestial(
    context: &GpuContext,
    source: &worldrunner_render_plan::RenderScene,
    samples: u32,
) -> Capture {
    let device = context.device();
    let pipelines =
        ScenePipelines::with_samples(device, wgpu::TextureFormat::Rgba16Float, samples).unwrap();
    let mut gpu = SceneGpu::upload(device, context.submission(), source, &pipelines).unwrap();
    let color = multisampled_target(device, wgpu::TextureFormat::Rgba16Float, samples);
    let depth = multisampled_target(device, wgpu::TextureFormat::Depth32Float, samples);
    let color = AttachmentView::new(ResourceRef::new("msaa color"), &color).unwrap();
    let depth = AttachmentView::new(ResourceRef::new("msaa depth"), &depth).unwrap();
    let frame = FrameUniform::new(&view(), source.effects()).unwrap();
    capture(context, |encoder, resolved, _| {
        let mut renderer = crate::compositor::FrameRenderer::new(device);
        crate::compositor::encode_scene(
            &mut renderer,
            device,
            encoder,
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame,
        )
        .unwrap();
        let _resolve = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("celestial acceptance resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color.view(),
                resolve_target: Some(resolved),
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    })
}

#[test]
fn celestial_alpha_coverage_composites_occupied_samples_once() {
    use worldrunner_render_plan::*;
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let masked = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: Some(TextureHandle::from_raw(0)),
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 500_000,
        },
        double_sided: true,
    })
    .unwrap()
    .with_alpha_factor(500_000)
    .unwrap()
    .with_alpha_to_coverage(true)
    .unwrap();
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(textures().to_vec(), vec![material(1), masked]).unwrap(),
        geometry: SceneGeometry::new(vec![quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects(0, false)),
    })
    .unwrap();
    let image = capture_celestial(&context, &source, 4);
    assert_eq!(
        image.rgb(4).map(f32::to_bits),
        [0.5_f32, 0.5, 1.5].map(f32::to_bits),
        "half white over blue radiance 2"
    );
    assert_eq!(
        image.rgb(0).map(f32::to_bits),
        [0.0_f32, 0.0, 2.0].map(f32::to_bits)
    );
}

#[test]
fn shadowed_scene_schedules_celestial_visibility_without_touching_scene_depth() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let source = scene(0, false, false);
    let mut gpu = crate::shadows::ShadowedScene::with_target(
        &context,
        &source,
        crate::shadows::DirectionalShadowSettings {
            focus_meters: glam::Vec3::ZERO,
            half_extent_meters: 10.0,
            depth_half_extent_meters: 10.0,
            resolution: 16,
            depth_bias: 0.0,
            normal_bias_meters: 0.0,
            refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
        },
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    let image = capture(&context, |encoder, color, depth| {
        let color =
            AttachmentView::new(ResourceRef::new("shadow acceptance color"), color.texture())
                .unwrap();
        let depth =
            AttachmentView::new(ResourceRef::new("shadow acceptance depth"), depth.texture())
                .unwrap();
        gpu.encode(&context, &view(), &color, &depth, encoder)
            .unwrap();
    });
    assert_eq!(
        image.rgb(4).map(f32::to_bits),
        [0.0, 1.0, 0.0].map(f32::to_bits)
    );
    for x in 0..9 {
        assert_eq!(image.depth(x).to_bits(), 1.0f32.to_bits());
    }
}

#[test]
fn allocation_plan_includes_visibility_only_when_meshes_are_present() {
    let sky = atmosphere(0);
    let required = |sky: &worldrunner_render_plan::SceneAtmosphere, samples| {
        super::allocations(sky, [9, 3], samples)
            .validate(&wgpu::Limits::default())
            .unwrap()
    };
    for samples in crate::msaa_tests::all_counts() {
        let with_meshes = required(&sky, samples);
        let mut bare = sky.clone();
        bare.meshes.clear();
        assert_eq!(
            with_meshes - required(&bare, samples),
            u64::from(9 * 3 * samples * 12) + crate::geometry::INSTANCE_STRIDE,
            "{samples}x"
        );
    }
}

fn multisampled_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    samples: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("multisampled celestial acceptance"),
        size: wgpu::Extent3d {
            width: 9,
            height: 3,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: samples,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}
