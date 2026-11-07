use super::*;
use crate::{draw::DrawCount, offscreen::OffscreenTarget};

#[test]
fn submitted_draw_statistics_track_visibility_without_counting_cancelled_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = scene(SceneAlphaMode::Opaque, false, 3);
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut target = target(&context);
    let handles: Vec<_> = gpu.instance_handles().collect();
    gpu.set_instance_visible(handles[1], false).unwrap();
    let frame = frame_from(pose());
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let mut encoder = context.submission().begin(context.device()).unwrap();
    crate::compositor::encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&target.color_attachment(), &target.depth_attachment()),
        frame,
    )
    .unwrap();
    assert_eq!(gpu.statistics().meshes, DrawCount::default());
    drop(encoder);
    assert_eq!(gpu.statistics().meshes, DrawCount::default());
    gpu.set_frame(context.queue(), frame);
    target.render(&context, &gpu, &pipelines).unwrap();
    let stats = gpu.statistics();
    assert_eq!(
        stats.meshes,
        DrawCount {
            calls: 2,
            instances: 2
        }
    );
    assert_eq!(stats.impostors, DrawCount::default());
    assert_eq!(stats.textures.resident_bytes, 8);
    assert_eq!(
        stats.mesh_buffers.resident_bytes,
        4 * crate::geometry::VERTEX_STRIDE + 6 * 4
    );
    assert_eq!(stats.decals_total, 0);
    assert_eq!(stats.decals_visible, 0);
    let first = gpu.take_statistics();
    assert_eq!(first.textures.uploaded_bytes, 8);
    assert_eq!(
        first.mesh_buffers.uploaded_bytes,
        first.mesh_buffers.resident_bytes
    );
    assert_eq!(gpu.take_statistics().textures.uploaded_bytes, 0);
    gpu.set_instance_visible(handles[1], true).unwrap();
    gpu.set_frame(context.queue(), frame);
    target.render(&context, &gpu, &pipelines).unwrap();
    assert_eq!(
        gpu.statistics().meshes,
        DrawCount {
            calls: 1,
            instances: 3
        }
    );
}

#[test]
fn draw_statistics_classify_impostors_and_count_lod_material_slots() {
    use worldrunner_render_plan::SceneFadeDirection;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let impostor = super::fade_handoff::handoff_scene(true, true, &[SceneFadeDirection::Keep]);
    let cases = [
        (
            impostor,
            frame_from(pose()),
            DrawCount {
                calls: 0,
                instances: 0,
            },
            DrawCount {
                calls: 1,
                instances: 1,
            },
        ),
        (
            super::lod_material::scene(true),
            super::lod_material::selection_frame(5_001),
            DrawCount {
                calls: 2,
                instances: 2,
            },
            DrawCount {
                calls: 0,
                instances: 0,
            },
        ),
    ];
    for (source, frame, meshes, impostors) in cases {
        let gpu =
            SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
        gpu.set_frame(context.queue(), frame);
        let mut target = target(&context);
        target.render(&context, &gpu, &pipelines).unwrap();
        assert_eq!(gpu.statistics().meshes, meshes);
        assert_eq!(gpu.statistics().impostors, impostors);
    }
}

#[test]
fn decal_statistics_apply_visibility_and_shader_capacity_at_submission() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut outside = super::decals::decal();
    outside.transform.position_millimeters = [100_000; 3];
    let mut decals = vec![super::decals::decal(); 33];
    decals.push(outside);
    let source = super::decals::source(false, false)
        .with_decals(decals)
        .unwrap();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let mut renderer = crate::compositor::FrameRenderer::new(context.device());
    let target = target(&context);
    let mut encoder = context.submission().begin(context.device()).unwrap();
    crate::compositor::encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&target.color_attachment(), &target.depth_attachment()),
        frame_from(pose()),
    )
    .unwrap();
    assert_eq!(gpu.statistics().decals_visible, 0);
    assert_eq!(gpu.statistics().decals_total, 0);
    encoder.finish().submit().unwrap();
    assert_eq!(gpu.statistics().decals_visible, 32);
    assert_eq!(gpu.statistics().decals_total, 34);
}

fn target(context: &GpuContext) -> OffscreenTarget {
    OffscreenTarget::new(
        context.device(),
        Viewport {
            width: 16,
            height: 16,
        },
    )
    .unwrap()
}

#[test]
fn raw_terrain_and_sky_draw_preserves_submitted_statistics() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let base = terrain_scene(1);
    let atmosphere = worldrunner_render_plan::SceneAtmosphere {
        celestial_composition: worldrunner_render_plan::SceneCelestialComposition::Inherit,
        celestial_exposure_micro: None,
        background_depth_policy:
            worldrunner_render_plan::SceneBackgroundDepthPolicy::ConstantDistance,
        scattering: None,
        horizon_radiance_micro: [1_000_000; 3],
        zenith_radiance_micro: [1_000_000; 3],
        gradient_exponent_micro: 1_000_000,
        sun_angular_radius_millidegrees: 0,
        fog: None,
        discs: vec![],
        meshes: vec![],
        background_distance_millimeters: 20_000,
    };
    let source = base
        .with_objects_and_effects(
            base.geometry().objects().to_vec(),
            Some(
                base.effects()
                    .unwrap()
                    .clone()
                    .with_atmosphere(atmosphere)
                    .unwrap(),
            ),
        )
        .unwrap();
    let pipelines = ScenePipelines::new(context.device(), TextureFormat::Rgba16Float);
    let gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame_from(pose()));
    let previous = gpu.statistics();
    let mut encoder = context.submission().begin(context.device()).unwrap();
    raw_draw(context.device(), &mut encoder, &gpu, &pipelines);
    encoder.finish().submit().unwrap();
    assert_eq!(gpu.statistics(), previous);
}

fn raw_draw(
    device: &Device,
    encoder: &mut CommandEncoder,
    gpu: &SceneGpu,
    pipelines: &ScenePipelines,
) {
    let views = [TextureFormat::Rgba16Float, TextureFormat::Depth32Float].map(|format| {
        device
            .create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: 16,
                    height: 16,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&TextureViewDescriptor::default())
    });
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &views[0],
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color::BLACK),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
            view: &views[1],
            depth_ops: Some(Operations {
                load: LoadOp::Clear(1.0),
                store: StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        ..Default::default()
    });
    gpu.draw(&mut pass, pipelines);
}
