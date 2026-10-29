//! The object pass writes the object-motion residual the TAA resolve reads.
//!
//! The scene is a 2 m quad on the ground plane. Its rigid wind moves it along +x, inside its own
//! plane, so the deformed surface stays on `z = 0` and the CPU model needs only a ray/plane
//! intersection per pixel. The quad's cutout texture discards some fragments, so only texels the
//! pass wrote are compared (at least 200 of them). Residuals are read back from the frame renderer's velocity target.
use super::foliage::foliage_scene;
use super::*;
use crate::compositor::FrameRenderer;
use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::{SceneAmbientOcclusion, SceneMaterialFeatures, SceneTint, SceneWind};
use worldrunner_render_post::{
    AntiAliasing, AttachmentView, FrameConfig, MsaaSamples, PostResolve, Readback, ResourceRef,
    ToneMap,
};

const HDR: TextureFormat = TextureFormat::Rgba16Float;
const AMPLITUDE_M: f32 = 0.5;
const PERIOD_MS: f32 = 1000.0;
/// Agreement with the CPU residual in previous-frame uv units: 0.064 px at 64 px. The sample sits
/// up to half a pixel from the ray the CPU model uses because the frame is jittered, and the
/// residual changes by about 1e-4 uv over that distance; half-float storage adds under 3e-5.
const TOLERANCE_UV: f32 = 1e-3;

fn windy() -> RenderScene {
    let wind = SceneWind::new(
        SceneDirection::new([1_000_000, 0, 0]).unwrap(),
        500,
        1000,
        0,
    )
    .unwrap();
    foliage_scene(
        Some(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: Some(wind),
            layer: None,
            translucency: None,
        }),
        SceneTint::WHITE,
        1_000_000,
    )
}

/// The scene with temporal antialiasing authored, so its resident post settings match a TAA renderer.
fn with_taa(scene: &RenderScene) -> (RenderScene, SceneEffects) {
    let effects = SceneEffects::new(SceneEffectsParts {
        anti_alias: SceneAntiAlias::taa(900_000),
        ..effect_parts()
    })
    .unwrap();
    let scene = RenderScene::new(SceneParts {
        resources: scene.resources().clone(),
        geometry: scene.geometry().clone(),
        terrain: scene.terrain().cloned(),
        effects: Some(effects.clone()),
    })
    .unwrap();
    (scene, effects)
}

fn still() -> RenderScene {
    foliage_scene(None, SceneTint::WHITE, 1_000_000)
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    match exponent {
        0 => sign * fraction * 2f32.powi(-24),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}

fn read_velocity(context: &GpuContext, texture: &wgpu::Texture) -> Vec<[f32; 2]> {
    let device = context.device();
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("velocity readback"),
        size: u64::from(TARGET * TARGET * 4),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TARGET * 4),
                rows_per_image: Some(TARGET),
            },
        },
        Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
    );
    context.queue().submit([encoder.finish()]);
    map_pixels(device, &buffer)
        .chunks_exact(4)
        .map(|texel| {
            [
                half_to_f32(u16::from_le_bytes([texel[0], texel[1]])),
                half_to_f32(u16::from_le_bytes([texel[2], texel[3]])),
            ]
        })
        .collect()
}

/// Draws one frame per `(seconds, pose)` and returns the velocity target of the last one.
fn velocity_after(
    context: &GpuContext,
    scene: &RenderScene,
    frames: &[(f32, CameraPose)],
) -> Vec<[f32; 2]> {
    let (scene, effects) = with_taa(scene);
    let pipelines = ScenePipelines::with_taa_velocity(context.device(), HDR, 1).unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    let mut renderer = FrameRenderer::hdr_antialiased(
        context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        AntiAliasing::new(MsaaSamples::X1, PostResolve::None),
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
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
    for &(seconds, pose) in frames {
        renderer.set_frame_time(seconds).unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        crate::compositor::encode_scene(
            &mut renderer,
            context.device(),
            &mut encoder,
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame_from(pose),
        )
        .unwrap();
        encoder.finish().submit().unwrap();
    }
    let texture = renderer
        .targets()
        .velocity_texture()
        .expect("a TAA renderer allocates the velocity target")
        .clone();
    read_velocity(context, &texture)
}

fn uv(view_projection: Mat4, point: Vec3) -> [f32; 2] {
    let clip = view_projection * Vec4::new(point.x, point.y, point.z, 1.0);
    [0.5 + 0.5 * clip.x / clip.w, 0.5 - 0.5 * clip.y / clip.w]
}

fn sway(milliseconds: f32) -> f32 {
    AMPLITUDE_M * (std::f32::consts::TAU * (milliseconds % PERIOD_MS) / PERIOD_MS).sin()
}

/// The residual of the ground-plane point seen through pixel `(x, y)`, and that point's offset
/// from the quad centre, with the quad displaced by `sway(now)` and previously by `sway(before)`.
fn expected(x: u32, y: u32, (before, now): (f32, f32), view_projection: Mat4) -> ([f32; 2], Vec3) {
    let [size, column, row] = [TARGET, x, y].map(|value| f32::from(u16::try_from(value).unwrap()));
    let ndc = |depth| {
        Vec4::new(
            (column + 0.5) / size * 2.0 - 1.0,
            1.0 - (row + 0.5) / size * 2.0,
            depth,
            1.0,
        )
    };
    let inverse = view_projection.inverse();
    let unproject = |depth| {
        let point = inverse * ndc(depth);
        point.truncate() / point.w
    };
    let (near, far) = (unproject(0.0), unproject(1.0));
    let ground = near + (far - near) * (near.z / (near.z - far.z));
    let previous = ground + Vec3::new(sway(before) - sway(now), 0.0, 0.0);
    let (a, b) = (uv(view_projection, previous), uv(view_projection, ground));
    (
        [a[0] - b[0], a[1] - b[1]],
        ground - Vec3::new(sway(now), 0.0, 0.0),
    )
}

fn assert_matches_cpu_residual(velocity: &[[f32; 2]], clocks: (f32, f32)) {
    let view_projection = frame_from(pose()).view_projection;
    let mut compared = 0;
    let mut largest = 0.0_f32;
    for y in 0..TARGET {
        for x in 0..TARGET {
            let (residual, local) = expected(x, y, clocks, view_projection);
            if local.x.abs() > 0.9 || local.y.abs() > 0.9 {
                continue;
            }
            let got = velocity[(y * TARGET + x) as usize];
            // The quad's cutout texture leaves holes whose texels keep the cleared zero.
            if got == [0.0, 0.0] {
                continue;
            }
            for lane in 0..2 {
                assert!(
                    (got[lane] - residual[lane]).abs() <= TOLERANCE_UV,
                    "pixel {x},{y}: got {got:?}, expected {residual:?}"
                );
            }
            largest = largest.max(residual[0].abs());
            compared += 1;
        }
    }
    assert!(compared > 200, "only {compared} pixels compared");
    assert!(largest > 0.01, "the residual was never large: {largest}");
}

#[test]
fn rigid_wind_writes_the_analytic_residual_at_two_clocks() {
    let context = context();
    for (first, second) in [(0.125, 0.25), (0.25, 0.5)] {
        let velocity = velocity_after(&context, &windy(), &[(first, pose()), (second, pose())]);
        assert_matches_cpu_residual(&velocity, (first * 1000.0, second * 1000.0));
    }
}

#[test]
fn a_static_object_under_a_moving_camera_has_exactly_zero_residual() {
    let context = context();
    let moved = CameraPose {
        eye_millimeters: [400, -2_000, 2_100],
        target_millimeters: [0, 0, 0],
    };
    let velocity = velocity_after(&context, &still(), &[(0.0, pose()), (0.0, moved)]);
    assert!(velocity.iter().all(|texel| *texel == [0.0, 0.0]));
}

#[test]
fn a_paused_clock_has_exactly_zero_residual_even_with_wind() {
    let context = context();
    let velocity = velocity_after(
        &context,
        &windy(),
        &[(0.25, pose()), (0.25, pose()), (0.25, pose())],
    );
    assert!(velocity.iter().all(|texel| *texel == [0.0, 0.0]));
}

#[test]
fn a_renderer_without_taa_allocates_no_velocity_target_and_refuses_velocity_pipelines() {
    let context = context();
    let plain = FrameRenderer::hdr_antialiased(
        &context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        AntiAliasing::new(MsaaSamples::X1, PostResolve::None),
        ToneMap::Reinhard,
        None,
    )
    .unwrap();
    assert!(plain.targets().velocity_texture().is_none());
    assert!(
        !ScenePipelines::with_samples(context.device(), HDR, 1)
            .unwrap()
            .writes_velocity()
    );
}

fn output_texture(device: &Device) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("end to end output"),
        size: Extent3d {
            width: TARGET,
            height: TARGET,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Draws one frame per clock, rotating frame slots as a host does, and returns the tone mapped
/// output of the last. `velocity` selects the velocity pipelines; without them the TAA resolve
/// reads zero residuals, which is the behaviour before the object pass wrote velocity.
fn output_after(
    context: &GpuContext,
    scene: &RenderScene,
    taa: Option<bool>,
    clocks: &[f32],
) -> Vec<u8> {
    let device = context.device();
    let (scene, effects, resolve) = match taa {
        Some(_) => {
            let (scene, effects) = with_taa(scene);
            (scene, effects, PostResolve::None)
        }
        None => (scene.clone(), effects(), PostResolve::None),
    };
    let pipelines = if taa == Some(true) {
        ScenePipelines::with_taa_velocity(device, HDR, 1).unwrap()
    } else {
        ScenePipelines::with_samples(device, HDR, 1).unwrap()
    };
    let mut gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
    let mut renderer = FrameRenderer::hdr_antialiased(
        context,
        [TARGET, TARGET],
        COLOR_FORMAT,
        AntiAliasing::new(MsaaSamples::X1, resolve),
        ToneMap::Reinhard,
        Some(&effects),
    )
    .unwrap();
    let output = output_texture(device);
    let view = output.create_view(&TextureViewDescriptor::default());
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
    for (index, &seconds) in clocks.iter().enumerate() {
        renderer.select_frame_slot(index % 3);
        renderer.set_frame_time(seconds).unwrap();
        let mut encoder = context.submission().begin(device).unwrap();
        crate::compositor::encode_scene(
            &mut renderer,
            device,
            &mut encoder,
            (&mut gpu, &pipelines),
            (&color, &depth),
            frame_from(pose()),
        )
        .unwrap();
        renderer.encode_output(&mut encoder, &view).unwrap();
        encoder.finish().submit().unwrap();
    }
    Readback::new(
        device,
        FrameConfig::new([TARGET, TARGET], COLOR_FORMAT, 1),
        1 << 20,
    )
    .unwrap()
    .read(device, context.submission(), &output)
    .unwrap()
}

fn mean_error(image: &[u8], reference: &[u8]) -> f64 {
    let sum: u32 = image
        .iter()
        .zip(reference)
        .map(|(a, b)| u32::from(a.abs_diff(*b)))
        .sum();
    f64::from(sum) / f64::from(u32::try_from(image.len()).unwrap()) / 255.0
}

#[test]
fn swaying_foliage_under_a_fixed_camera_resolves_closer_to_the_unjittered_frame_with_velocity() {
    // 16 frames of rigid sway at 31.25 ms steps (about 1.5 px a frame at the quad), error measured
    // as the mean absolute difference of the 8-bit output from the unjittered frame at the last clock.
    let context = context();
    let clocks: Vec<f32> = (1..=16_u8).map(|step| f32::from(step) * 0.03125).collect();
    let scene = windy();
    let reference = output_after(&context, &scene, None, &clocks[15..]);
    let without = mean_error(
        &output_after(&context, &scene, Some(false), &clocks),
        &reference,
    );
    let with = mean_error(
        &output_after(&context, &scene, Some(true), &clocks),
        &reference,
    );
    // Measured on lavapipe: 0.01417 without velocity (the previous behaviour) and 0.00856 with it,
    // a ratio of 0.60. The bound leaves margin for driver differences and still fails if the
    // resolve ignores the residual.
    assert!(
        with < 0.75 * without,
        "with velocity {with}, without {without}"
    );
}
