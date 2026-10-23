#[path = "msaa.rs"]
mod msaa;
#[path = "numerical.rs"]
mod numerical;
#[path = "regressions.rs"]
mod regressions;
use super::*;
use crate::device::GpuContext;
use worldrunner_render_post::{Readback, ToneMap};

fn settings() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 256,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
    }
}
fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}
fn frame() -> FrameUniform {
    FrameUniform {
        view: glam::Mat4::IDENTITY,
        view_projection: glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.1, 200.0),
        camera_position: glam::Vec3::ZERO,
        camera_position_millimeters: [0; 3],
        sun_direction: glam::Vec3::Z,
        sun_irradiance: glam::Vec3::ZERO,
        ambient_sky: glam::Vec3::ZERO,
        ambient_ground: glam::Vec3::ZERO,
        exposure: 1.0,
        unlit: false,
    }
}
fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(context.device(), context.queue(), texture)
    .unwrap()
}
fn half(value: f32) -> u16 {
    if value == 0.0 {
        return 0;
    }
    let bits = value.to_bits();
    let sign = u16::try_from((bits >> 16) & 0x8000).unwrap();
    let exponent = i32::try_from((bits >> 23) & 255).unwrap() - 127 + 15;
    assert!((1..31).contains(&exponent));
    sign | (u16::try_from(exponent).unwrap() << 10)
        | u16::try_from((bits & 0x007f_ffff) >> 13).unwrap()
}
fn upload(context: &GpuContext, texture: &wgpu::Texture, values: &[f32]) {
    let bytes: Vec<u8> = if texture.format() == wgpu::TextureFormat::R32Float {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    } else {
        values
            .iter()
            .flat_map(|value| half(*value).to_le_bytes())
            .collect()
    };
    let bpp = if texture.format() == wgpu::TextureFormat::R32Float {
        4
    } else {
        8
    };
    context.queue().write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(texture.width() * bpp),
            rows_per_image: Some(texture.height()),
        },
        texture.size(),
    );
}
#[test]
fn settings_validate_enabled_and_disabled_requests() {
    assert!(validate_settings(&settings()).is_ok());
    for invalid in [
        SceneSsr {
            max_steps: 0,
            ..settings()
        },
        SceneSsr {
            max_steps: 257,
            ..settings()
        },
        SceneSsr {
            thickness_millimeters: 0,
            ..settings()
        },
        SceneSsr {
            thickness_millimeters: 1001,
            ..settings()
        },
        SceneSsr {
            roughness_cutoff_micro: 0,
            ..settings()
        },
        SceneSsr {
            roughness_cutoff_micro: 1_000_001,
            ..settings()
        },
        SceneSsr {
            resolution_scale_micro: 750_000,
            ..settings()
        },
    ] {
        for enabled in [true, false] {
            assert!(matches!(
                validate_settings(&SceneSsr { enabled, ..invalid }),
                Err(FrameError::SsrSettings(_))
            ));
        }
    }
}
#[test]
fn gpu_reflection_resources_compile_and_half_resolution_includes_odd_edges() {
    let context = context();
    for size in [[65, 49], [1, 7], [7, 1]] {
        let mut screen = Ssao::new(context.device(), size, neutral_ao()).unwrap();
        let ssr = Ssr::new(
            context.device(),
            FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 4),
            SceneSsr {
                resolution_scale_micro: 500_000,
                ..settings()
            },
            &mut screen,
        )
        .unwrap();
        assert_eq!(
            [ssr.reflections[0].width(), ssr.reflections[0].height()],
            size.map(|side| side.div_ceil(2))
        );
        assert!(screen.geometry.reflection.is_some());
    }
}

fn upload_hierarchy(context: &GpuContext, ssr: &Ssr, size: [u32; 2], depths: Vec<f32>) {
    let mut level_size = size;
    let mut reduced = depths;
    let mut hierarchy_values = vec![reduced.clone()];
    while level_size != [1, 1] {
        let next = level_size.map(|side| side.div_ceil(2));
        let mut values = vec![f32::INFINITY; usize::try_from(next[0] * next[1]).unwrap()];
        for y in 0..level_size[1] {
            for x in 0..level_size[0] {
                let output = usize::try_from(y / 2 * next[0] + x / 2).unwrap();
                values[output] =
                    values[output].min(reduced[usize::try_from(y * level_size[0] + x).unwrap()]);
            }
        }
        reduced = values;
        level_size = next;
        hierarchy_values.push(reduced.clone());
    }
    for (index, texture) in [0usize, 2, 4, 6].into_iter().zip(&ssr.hierarchy) {
        upload(
            context,
            texture,
            &hierarchy_values[index.min(hierarchy_values.len() - 1)],
        );
    }
}
fn draw_trace(
    context: &GpuContext,
    ssr: &Ssr,
    frame: FrameUniform,
    opaque: &wgpu::Texture,
) -> Vec<u8> {
    let output = AttachmentView::new(ResourceRef::new("scene color"), opaque).unwrap();
    let mut feature = ReflectionFeature {
        ssr,
        frame,
        color: output,
        groups: Vec::new(),
    };
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let mut prepare = PrepareContext::new(context.device(), &mut encoder);
    feature.commit(&mut prepare);
    prepare.finish();
    let mut inputs = vec![
        opaque.create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.normals
            .create_view(&wgpu::TextureViewDescriptor::default()),
        ssr.material
            .create_view(&wgpu::TextureViewDescriptor::default()),
    ];
    inputs.extend(
        ssr.hierarchy
            .iter()
            .map(|texture| texture.create_view(&wgpu::TextureViewDescriptor::default())),
    );
    let group = gpu::group(context.device(), &ssr.trace, Some(&ssr.uniform), &inputs);
    worldrunner_render_post::fullscreen_pass(
        &mut encoder,
        Some("reflection reference"),
        wgpu::RenderPassColorAttachment {
            view: &ssr.reflections[0].create_view(&wgpu::TextureViewDescriptor::default()),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        },
        &ssr.trace,
        &[&group],
    );
    context.queue().submit([encoder.finish()]);
    read(context, &ssr.reflections[0])
}
fn mirror_data(
    frame: FrameUniform,
    roughness: f32,
    target_uv: glam::Vec2,
    occluder: bool,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let origin_uv = glam::Vec2::new(0.5, 0.7);
    let position = |uv: glam::Vec2, depth: f32| {
        let v = frame.view_projection.inverse()
            * glam::Vec4::new(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
        v.truncate() * (-depth / v.z)
    };
    let origin = position(origin_uv, 3.0);
    let target = position(target_uv, 5.0);
    let direction = (target - origin).normalize();
    let normal = (direction - origin.normalize()).normalize();
    let reflected = origin.normalize() - 2.0 * origin.normalize().dot(normal) * normal;
    let intersection = origin + reflected * ((-5.0 - origin.z) / reflected.z);
    let projected = frame.view_projection.project_point3(intersection);
    let expected_uv = projected.truncate() * glam::Vec2::new(0.5, -0.5) + glam::Vec2::splat(0.5);
    assert!(
        expected_uv.distance(target_uv) < 0.0001,
        "CPU mirror-plane intersection"
    );
    let mut depths = vec![5.0; 65 * 65];
    let mut normals = [0.0, 0.0, 1.0, 1.0].repeat(65 * 65);
    let mut materials = [0.0, 0.0, 0.0, 1.0].repeat(65 * 65);
    let mut colors = vec![0.0; 65 * 65 * 4];
    for y in 0..65u32 {
        for x in 0..65u32 {
            let i = usize::try_from(y * 65 + x).unwrap();
            let uv = glam::Vec2::new(gpu::scalar(x) + 0.5, gpu::scalar(y) + 0.5) / 65.0;
            if (uv - origin_uv).length() < 0.08 {
                let ray = position(uv, 1.0);
                depths[i] = -ray.z * origin.dot(normal) / ray.dot(normal);
                normals[i * 4..i * 4 + 4].copy_from_slice(&[normal.x, normal.y, normal.z, 1.0]);
                materials[i * 4..i * 4 + 4].copy_from_slice(&[0.04, 0.04, 0.04, roughness]);
            }
            if (uv - target_uv).length() < 0.07 {
                colors[i * 4] = 1.0;
            }
            colors[i * 4 + 3] = 1.0;
            if occluder && uv.y > 0.50 && uv.y < 0.54 {
                depths[i] = 1.0;
            }
        }
    }
    (depths, normals, materials, colors)
}
fn trace_fixture(
    context: &GpuContext,
    roughness: f32,
    target_uv: glam::Vec2,
    occluder: bool,
) -> Vec<u8> {
    trace_fixture_normal(
        context,
        roughness,
        target_uv,
        occluder,
        [0.0, 0.0, 1.0, 1.0],
    )
}
fn trace_fixture_normal(
    context: &GpuContext,
    roughness: f32,
    target_uv: glam::Vec2,
    occluder: bool,
    target_normal: [f32; 4],
) -> Vec<u8> {
    let size = [65, 65];
    let mut screen = Ssao::new(context.device(), size, neutral_ao()).unwrap();
    let ssr = Ssr::new(
        context.device(),
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        settings(),
        &mut screen,
    )
    .unwrap();
    let frame = frame();
    let (depths, mut normals, materials, colors) =
        mirror_data(frame, roughness, target_uv, occluder);
    for (index, depth) in depths.iter().enumerate() {
        if depth.to_bits() == 5.0f32.to_bits() {
            normals[index * 4..index * 4 + 4].copy_from_slice(&target_normal);
        }
    }
    upload(context, &ssr.normals, &normals);
    upload(context, &ssr.material, &materials);
    let opaque =
        crate::ssao::passes::texture(context.device(), size, wgpu::TextureFormat::Rgba16Float);
    upload(context, &opaque, &colors);
    upload_hierarchy(context, &ssr, size, depths);
    draw_trace(context, &ssr, frame, &opaque)
}

#[test]
fn gpu_mirror_reflects_cpu_reference_object_and_rough_surfaces_do_not() {
    let context = context();
    let target = glam::Vec2::new(0.65, 0.35);
    let reflection = trace_fixture(&context, 0.0, target, false);
    let center = (45 * 65 + 32) * 8;
    assert!(
        u16::from_le_bytes(reflection[center..center + 2].try_into().unwrap()) > 0,
        "known red object must reflect at the CPU receiver position"
    );
    assert!(
        trace_fixture(&context, 0.75, target, false)
            .iter()
            .all(|value| *value == 0)
    );
    assert!(
        trace_fixture(&context, 0.0, target, true)[center..center + 8]
            .iter()
            .all(|value| *value == 0),
        "thin foreground occluder must not reveal the red object behind it"
    );
}
#[test]
fn gpu_screen_edge_fade_and_ray_miss() {
    let context = context();
    let middle = trace_fixture(&context, 0.0, glam::Vec2::new(0.65, 0.35), false);
    let edge = trace_fixture(&context, 0.0, glam::Vec2::new(0.98, 0.35), false);
    let missing = trace_fixture(&context, 0.0, glam::Vec2::new(1.3, 0.35), false);
    let pixel = (45 * 65 + 32) * 8;
    let red = |bytes: &[u8]| u16::from_le_bytes(bytes[pixel..pixel + 2].try_into().unwrap());
    assert!(red(&middle) > red(&edge));
    assert!(red(&edge) > 0);
    assert_eq!(red(&missing), 0);
}

fn renderer(context: &GpuContext) -> crate::compositor::FrameRenderer {
    let mut renderer = crate::compositor::FrameRenderer::hdr(
        context,
        [65, 65],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    renderer.ssao = Some(Ssao::new(context.device(), [65, 65], neutral_ao()).unwrap());
    renderer.ssr = Some(
        Ssr::new(
            context.device(),
            renderer.targets().config(),
            settings(),
            renderer.ssao.as_mut().unwrap(),
        )
        .unwrap(),
    );
    renderer
}
fn attachments(
    renderer: &crate::compositor::FrameRenderer,
) -> (AttachmentView, AttachmentView, PassDeclaration) {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(crate::compositor::attachment(
        color.resource(),
        &color,
        false,
    ));
    opaque.depth = Some(crate::compositor::attachment(
        depth.resource(),
        &depth,
        true,
    ));
    (color, depth, opaque)
}
#[test]
fn gpu_production_plan_uses_requested_snapshots_and_rejects_missing_inputs() {
    let context = context();
    let mut renderer = renderer(&context);
    let (color, depth, mut opaque) = attachments(&renderer);
    for slot in 0..2 {
        renderer.select_frame_slot(slot);
        opaque.color.as_mut().unwrap().load = AttachmentLoad::ClearColor(wgpu::Color {
            r: if slot == 0 { 0.25 } else { 0.5 },
            g: 0.125,
            b: 0.0625,
            a: 1.0,
        });
        let mut encoder = context
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        renderer
            .encode_frame(
                context.device(),
                &mut encoder,
                (&[opaque.clone()], &[color.clone(), depth.clone()]),
                &mut [],
                (frame(), None),
            )
            .unwrap();
        context.queue().submit([encoder.finish()]);
    }
    context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert_ne!(
        renderer.snapshots.as_ref().unwrap()[0].color,
        renderer.snapshots.as_ref().unwrap()[1].color
    );
    assert!(renderer.post_water_depth().is_some());
    let snapshots = renderer.snapshots.as_ref().unwrap();
    assert_eq!(
        &read(&context, &snapshots[0].color)[..2],
        &half(0.25).to_le_bytes()
    );
    assert_eq!(
        &read(&context, &snapshots[1].color)[..2],
        &half(0.5).to_le_bytes()
    );
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    opaque.depth = None;
    assert!(matches!(
        renderer.encode_frame(
            context.device(),
            &mut encoder,
            (&[opaque], &[color]),
            &mut [],
            (frame(), None)
        ),
        Err(FrameError::AttachmentContract(PassSlot::Opaque))
    ));
    context.queue().submit([encoder.finish()]);
}
