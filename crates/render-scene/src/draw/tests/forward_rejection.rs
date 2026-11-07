//! Frozen forward reference, compared before any resolve across every sample and MRT.
use super::*;

#[path = "forward_fixtures.rs"]
pub(super) mod fixtures;
#[path = "forward_samples.rs"]
pub(super) mod samples;
#[path = "forward_shadow.rs"]
mod shadow;

#[test]
fn forward_card_terms_match_frozen_colour_and_coverage() {
    use crate::compositor::terms::CaptureTerm;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut survivors = 0;
    for (name, scene, pose) in fixtures::scenes() {
        for shadows in [0, 1, 4] {
            for term in [
                CaptureTerm::Albedo,
                CaptureTerm::SunVisibility,
                CaptureTerm::DirectDiffuse,
                CaptureTerm::AmbientDiffuse,
                CaptureTerm::EnvironmentSpecular,
                CaptureTerm::SunSpecular,
            ] {
                let old = capture(
                    &context,
                    (&scene, pose),
                    (1, shadows, true),
                    (false, Some(term)),
                );
                let new = capture(
                    &context,
                    (&scene, pose),
                    (1, shadows, false),
                    (false, Some(term)),
                );
                for (index, (expected, got)) in old.iter().zip(&new).enumerate() {
                    assert!(
                        expected == got,
                        "{name}: {term:?}, shadows={shadows}, attachment={index}, mismatching bytes={}",
                        expected.iter().zip(got).filter(|(a, b)| a != b).count()
                    );
                }
                survivors += new[4]
                    .chunks_exact(16)
                    .filter(|sample| sample[..4] != 1.0_f32.to_le_bytes())
                    .count();
            }
        }
    }
    assert!(survivors > 0, "term fixtures must render surviving samples");
}

fn reference(source: &str) -> String {
    source
        .replace(
            concat!(include_str!("../../shader/forward_cards.wgsl"), "\n"),
            "",
        )
        .replace(
            include_str!("../../shader/environment.wgsl"),
            include_str!("forward_reference/environment.wgsl"),
        )
        .replace(
            include_str!("../../shader/shadows.wgsl"),
            include_str!("forward_reference/shadows.wgsl"),
        )
        .replace(
            include_str!("../../shader/unshadowed.wgsl"),
            include_str!("forward_reference/unshadowed.wgsl"),
        )
}

pub(super) fn moving_frame(pose: CameraPose, effects: Option<&SceneEffects>) -> FrameUniform {
    let mut frame = FrameUniform::new(&view(pose), effects).unwrap();
    frame.animation_time_milliseconds = 250;
    frame.temporal = Some(crate::frame::temporal::TemporalView {
        unjittered_view_projection: frame.view_projection,
        previous_view_projection: frame.view_projection,
        jitter_pixels: [0.0; 2],
        previous_jitter_pixels: [0.0; 2],
        history_valid: true,
        previous_animation_time_ms: 125,
        previous_camera_position: frame.camera_position + glam::Vec3::new(0.15, 0.0, 0.0),
    });
    frame
}

fn capture(
    context: &GpuContext,
    shot: (&RenderScene, CameraPose),
    config: (u32, u32, bool),
    output: (bool, Option<crate::compositor::terms::CaptureTerm>),
) -> Vec<Vec<u8>> {
    let (count, shadows, frozen) = config;
    let device = context.device();
    let stock = crate::shadows::capture_sources(shadows != 0);
    let object = if frozen {
        reference(stock.0)
    } else {
        stock.0.to_owned()
    };
    let object = if let Some(term) = output.1 {
        crate::compositor::terms::term_sources((&object, stock.1), term).0
    } else {
        object
    };
    let object = if output.0 {
        object.replace("shaded.rgb, coverage_alpha", "vec3<f32>(shaded.a, select(0.0, 1.0, shaded.a < scene_alpha_cutoff(input, false)), coverage_alpha), coverage_alpha")
    } else {
        object
    };
    let (object, terrain) = crate::ssr::forward::velocity_sources((&object, stock.1), true, true);
    let (shadow_layout, shadow_group) = shadow::binding(context, shadows);
    let screen_layout = crate::ssr::forward::layout(device);
    let pipelines = ScenePipelines::with_screen_space(
        device,
        (TextureFormat::Rgba16Float, count),
        &object,
        &terrain,
        Some(&shadow_layout),
        Some(&screen_layout),
    );
    let scene = lit(shot.0, true);
    let gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
    let group = samples::screen_group(context, &screen_layout);
    let targets = [
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba16Float,
        TextureFormat::Rg16Float,
        DEPTH_FORMAT,
    ]
    .map(|format| samples::attachment(device, format, count));
    let views = targets
        .each_ref()
        .map(|target| target.create_view(&TextureViewDescriptor::default()));
    let colors: Vec<_> = views[..4]
        .iter()
        .map(|view| {
            Some(RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })
        })
        .collect();
    let mut encoder = context.submission().begin(device).unwrap();
    let mut prepare = worldrunner_render_post::PrepareContext::new(device, &mut encoder).unwrap();
    gpu.commit_frame(&mut prepare, moving_frame(shot.1, scene.effects()));
    prepare.finish();
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("forward differential without resolve"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &views[4],
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(2, &shadow_group, &[]);
        pass.set_bind_group(3, &group, &[]);
        gpu.draw_shadowed(&mut pass, &pipelines, &shadow_group);
    }
    encoder.finish().submit().unwrap();
    targets
        .iter()
        .map(|target| samples::read(context, target))
        .collect()
}

#[test]
fn forward_cards_match_frozen_per_sample_colour_depth_reflection_and_velocity() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut survivors = 0;
    let mut partial = 0;
    let mut moving = 0;
    let mut reflective = 0;
    for (name, scene, pose) in fixtures::scenes() {
        for count in [1, 4] {
            for shadows in [0, 1, 4] {
                let old = capture(
                    &context,
                    (&scene, pose),
                    (count, shadows, true),
                    (false, None),
                );
                let new = capture(
                    &context,
                    (&scene, pose),
                    (count, shadows, false),
                    (false, None),
                );
                for (index, (expected, got)) in old.iter().zip(&new).enumerate() {
                    assert!(
                        expected == got,
                        "{name}: samples={count}, shadows={shadows}, attachment={index}, mismatching bytes={}",
                        expected.iter().zip(got).filter(|(a, b)| a != b).count()
                    );
                }
                moving += new[3]
                    .chunks_exact(16)
                    .filter(|pixel| pixel[..8].iter().any(|byte| *byte != 0))
                    .count();
                reflective += new[1]
                    .chunks_exact(16)
                    .filter(|pixel| pixel[..12].iter().any(|byte| *byte != 0))
                    .count();
                if name == "fully transparent rejected" {
                    assert!(
                        new[4]
                            .chunks_exact(16)
                            .all(|sample| sample[..4] == 1.0_f32.to_le_bytes())
                    );
                    for attachment in &new[..4] {
                        assert!(
                            attachment
                                .chunks_exact(16)
                                .all(|sample| sample[..8].iter().all(|byte| *byte == 0))
                        );
                    }
                }
                for pixel in new[4].chunks_exact(16 * count as usize) {
                    let covered = pixel
                        .chunks_exact(16)
                        .filter(|sample| sample[..4] != 1.0_f32.to_le_bytes())
                        .count();
                    survivors += covered;
                    partial += usize::from(covered > 0 && covered < count as usize);
                }
            }
        }
    }
    assert!(survivors > 0);
    assert!(partial > 0, "fixture must exercise partial sample masks");
    assert!(moving > 0, "fixture must publish nonzero motion residuals");
    assert!(
        reflective > 0,
        "fixture must publish nonzero environment radiance"
    );
}

#[test]
fn frozen_a2c_fixture_retains_below_cutoff_and_raw_zero_alpha_samples() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut below = 0;
    let mut zero = 0;
    for (_, scene, pose) in fixtures::scenes() {
        let data = capture(&context, (&scene, pose), (4, 0, true), (true, None));
        for (colour, depth) in data[0].chunks_exact(16).zip(data[4].chunks_exact(16)) {
            if depth[..4] == 1.0_f32.to_le_bytes() {
                continue;
            }
            below += usize::from(colour[4..8] == 1.0_f32.to_le_bytes());
            zero += usize::from(colour[..4] == 0.0_f32.to_le_bytes());
        }
    }
    assert!(below > 0, "below-cutoff alpha must retain nonzero coverage");
    assert!(
        zero > 0,
        "raw zero alpha at a zero cutoff must retain nonzero coverage"
    );
}
