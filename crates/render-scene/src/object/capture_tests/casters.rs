//! The object shadow casters: depth-only passes over the light frames of four cascades, drawn
//! through `SceneGpu::draw_casters_prepared` with the scene's caster programs and through the
//! frozen fixed casters over the ranges each cascade selects, with the depth texels read back
//! and compared byte for byte.
//!
//! The scene is the prepass captures' frame: an opaque mesh, a relief impostor, a masked
//! double-sided card and a foliage card under wind. The light looks at the cards' faces. One
//! cascade covers every card, two cover parts of the frame, and one misses every card, so the
//! cascades' selections differ and an empty selection is pinned too.

use glam::{Mat4, Vec3};
use wgpu::TextureFormat;

use super::{Rig, Target, digest, prepass, render_with_depth, scenes};
use crate::dynamic::capture_tests::context;
use crate::renderer::legacy_objects::{caster_pipelines, caster_source, record_objects};
use crate::shadows::DirectionalShadowSettings;

/// An orthographic light frame looking along +Y at the square of `half` metres about `centre`
/// in the x-z plane, where the cards stand.
fn map(centre: (f32, f32), half: f32) -> Mat4 {
    let target = Vec3::new(centre.0, 0.0, centre.1);
    glam::camera::rh::proj::directx::orthographic(-half, half, -half, half, 0.5, 12.0)
        * glam::camera::rh::view::look_at_mat4(target - Vec3::new(0.0, 6.0, 0.0), target, Vec3::Z)
}

/// Every card, the left column, the upper right card, and a square to the side of all of them.
fn cascades() -> [(Mat4, &'static str, bool); 4] {
    [
        (map((0.0, 1.75), 2.8), "all", true),
        (map((-1.4, 1.75), 1.2), "left", true),
        (map((1.4, 2.5), 1.0), "upper-right", true),
        (map((9.0, 1.75), 1.0), "none", false),
    ]
}

fn settings() -> DirectionalShadowSettings {
    DirectionalShadowSettings {
        focus_meters: Vec3::ZERO,
        half_extent_meters: 4.0,
        depth_half_extent_meters: 4.0,
        resolution: 64,
        depth_bias: 0.0,
        normal_bias_meters: 0.0,
        refresh_policy: crate::shadows::CascadeRefreshPolicy::default(),
    }
}

/// The depth of one cascade's map drawn three ways: by the scene's casters, by the frozen casters,
/// and by nothing, with how many draw calls the frozen casters recorded.
struct Maps {
    drawn: Vec<Vec<u8>>,
    fixed: Vec<Vec<u8>>,
    untouched: Vec<Vec<u8>>,
    calls: u64,
}

fn capture(
    rig: &mut Rig<'_>,
    (live, frozen): (&crate::object::program::Context, &[wgpu::RenderPipeline]),
    (cascade, group): ((u32, Mat4), &wgpu::BindGroup),
) -> Maps {
    let context = rig.context;
    let target = Target {
        formats: &[],
        samples: 1,
    };
    let depth = TextureFormat::Depth32Float;
    rig.prime();
    let drawn = render_with_depth(context, target, depth, &mut rig.gpu, |pass, gpu| {
        drop(gpu.draw_casters_prepared(pass, group, live, cascade));
    });
    rig.prime();
    let mut calls = 0;
    let fixed = render_with_depth(context, target, depth, &mut rig.gpu, |pass, gpu| {
        calls = record_objects(
            pass,
            frozen,
            &gpu.legacy_caster_objects(cascade, group.clone()),
        )
        .calls;
    });
    rig.prime();
    let untouched = render_with_depth(context, target, depth, &mut rig.gpu, |_, _| {});
    Maps {
        drawn,
        fixed,
        untouched,
        calls,
    }
}

#[test]
fn object_casters_match_the_fixed_casters_byte_for_byte_in_every_cascade() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    if crate::shader::ENVIRONMENT_WGSL
        == crate::renderer::legacy_objects::compose(
            crate::renderer::legacy_objects::Recipe::default(),
        )
    {
        assert!(
            caster_source() == crate::shadows::depth_source(),
            "the frozen caster composition differs from the live one"
        );
    }
    let pipelines = crate::renderer::ScenePipelines::new(device, TextureFormat::Rgba16Float);
    let mut rig = Rig::moving(
        &context,
        &pipelines,
        &scenes::scene(&prepass::specs()),
        false,
    );
    let sampling = worldrunner_render_plan::SceneShadowSampling::default();
    let (live, configuration) = crate::shadows::object_casters(&pipelines, settings(), sampling);
    let start = pipelines.object_runtime_stats();
    rig.gpu.prepare_object_context(&live).unwrap();
    let before = pipelines.object_runtime_stats();
    let frozen = caster_pipelines(device, &pipelines, configuration);
    let adapter = context.adapter().get_info().name;
    let mut compared = 0;
    for (index, (matrix, name, covers)) in cascades().into_iter().enumerate() {
        let cascade = (u32::try_from(index).unwrap(), matrix);
        let buffer = crate::upload::upload_buffer(
            device,
            context.queue(),
            "capture caster frame",
            wgpu::BufferUsages::UNIFORM,
            &crate::shadows::light_frame(&rig.frame, matrix).to_bytes(),
        );
        let group = crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer);
        let Maps {
            drawn,
            fixed,
            untouched,
            calls,
        } = capture(&mut rig, (&live, &frozen), (cascade, &group));
        assert!(
            drawn == fixed,
            "caster-{name}: the depth differs from the fixed casters' output (if \
             the_live_object_wgsl_equals_the_frozen_snapshot fails too, copy the live WGSL into \
             object/fixtures)"
        );
        if covers {
            assert!(calls > 0, "caster-{name}: the fixed casters drew nothing");
            assert!(drawn != untouched, "caster-{name}: no card reached the map");
        } else {
            assert_eq!(
                calls, 0,
                "caster-{name}: a cascade off the cards selected one"
            );
            assert!(
                drawn == untouched,
                "caster-{name}: the map changed with nothing selected"
            );
        }
        println!(
            "OBJECT-CAPTURE adapter={adapter:?} config=caster-{name} scene=casters mask=0 attachment=depth calls={calls} bytes={} hash={:016x}",
            drawn[0].len(),
            digest(&drawn[0])
        );
        compared += 1;
    }
    assert_eq!(compared, 4);
    assert!(
        before.pipelines_built > start.pipelines_built,
        "preparing the casters built their pipelines on the program runtime"
    );
    assert_eq!(
        pipelines.object_runtime_stats(),
        before,
        "the cascades found every caster prepared"
    );
    println!("OBJECT-CAPTURE-SUMMARY set=casters compared={compared} skipped=0");
}
