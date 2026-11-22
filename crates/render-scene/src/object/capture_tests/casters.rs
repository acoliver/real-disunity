//! The object shadow casters: depth-only passes over the light frames of four cascades, drawn
//! through `SceneGpu::draw_casters_prepared` with the scene's caster programs over the ranges
//! each cascade selects, with the depth texels read back and compared with an untouched map.
//!
//! The scene is the prepass captures' frame: an opaque mesh, a relief impostor, a masked
//! double-sided card and a foliage card under wind. The light looks at the cards' faces. One
//! cascade covers every card, two cover parts of the frame, and one misses every card, so the
//! cascades' selections differ and an empty selection is pinned too.

use glam::{Mat4, Vec3};
use wgpu::TextureFormat;

use super::{Rig, Target, digest, prepass, render_with_depth, scenes};
use crate::dynamic::capture_tests::context;
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

/// The depth of one cascade's map drawn two ways: by the scene's casters and by nothing.
struct Maps {
    drawn: Vec<Vec<u8>>,
    untouched: Vec<Vec<u8>>,
}

fn capture(
    rig: &mut Rig<'_>,
    live: &crate::object::program::Context,
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
    let untouched = render_with_depth(context, target, depth, &mut rig.gpu, |_, _| {});
    Maps { drawn, untouched }
}

#[test]
fn object_casters_write_the_depth_of_the_cascades_that_cover_cards() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let pipelines =
        crate::renderer::ScenePipelines::new(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut rig = Rig::moving(
        &context,
        &pipelines,
        &scenes::scene(&prepass::specs()),
        false,
    );
    let sampling = worldrunner_render_plan::SceneShadowSampling::default();
    let live = crate::shadows::object_casters(&pipelines, settings(), sampling);
    let start = pipelines.object_runtime_stats();
    rig.gpu.prepare_object_context(&live).unwrap();
    let before = pipelines.object_runtime_stats();
    let adapter = context.adapter().get_info().name;
    let mut compared = 0;
    for (index, (matrix, name, covers)) in cascades().into_iter().enumerate() {
        let cascade = (u32::try_from(index).unwrap(), matrix);
        let buffer = worldrunner_render_gpu::resource::upload_buffer(
            device,
            context.queue(),
            "capture caster frame",
            wgpu::BufferUsages::UNIFORM,
            &crate::shadows::light_frame(&rig.frame, matrix).to_bytes(),
        );
        let group = crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer);
        let Maps { drawn, untouched } = capture(&mut rig, &live, (cascade, &group));
        if covers {
            assert!(drawn != untouched, "caster-{name}: no card reached the map");
        } else {
            assert!(
                drawn == untouched,
                "caster-{name}: the map changed with nothing selected"
            );
        }
        println!(
            "OBJECT-CAPTURE adapter={adapter:?} config=caster-{name} scene=casters mask=0 attachment=depth bytes={} hash={:016x}",
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

/// A light frame looking down -Z at the ground plane the skinning ribbon lies on.
fn ribbon_light() -> Mat4 {
    glam::camera::rh::proj::directx::orthographic(-1.5, 1.5, -1.5, 1.5, 0.5, 12.0)
        * glam::camera::rh::view::look_at_mat4(Vec3::new(0.0, 0.0, 6.0), Vec3::ZERO, Vec3::Y)
}

/// The caster depth of `scene` in one cascade over the ribbon.
fn ribbon_map(
    context: &crate::device::GpuContext,
    scene: &worldrunner_render_plan::RenderScene,
) -> Vec<u8> {
    let device = context.device();
    let pipelines =
        crate::renderer::ScenePipelines::new(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut rig = Rig::moving(context, &pipelines, scene, false);
    let sampling = worldrunner_render_plan::SceneShadowSampling::default();
    let live = crate::shadows::object_casters(&pipelines, settings(), sampling);
    rig.gpu.prepare_object_context(&live).unwrap();
    let matrix = ribbon_light();
    let buffer = worldrunner_render_gpu::resource::upload_buffer(
        device,
        context.queue(),
        "skinned caster frame",
        wgpu::BufferUsages::UNIFORM,
        &crate::shadows::light_frame(&rig.frame, matrix).to_bytes(),
    );
    let group = crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer);
    capture(&mut rig, &live, ((0, matrix), &group))
        .drawn
        .swap_remove(0)
}

/// Skinned objects cast through the caster programs: at the rest pose the map is the rigid
/// ribbon's, and moving joint 1 moves the depth it writes.
#[test]
fn skinned_casters_write_the_posed_depth() {
    let _session = crate::test_support::SceneTestSession::acquire();
    // The ribbon's material samples a BC1 texture, so this needs the BC-capable test device.
    let context = crate::device::GpuContext::shared_for_test()
        .expect("GPU acceptance requires a real adapter with BC support");
    let skinning = crate::draw::tests::skinning::posed;
    let rest = ribbon_map(&context, &skinning(0));
    assert_eq!(
        rest,
        ribbon_map(&context, &crate::draw::tests::skinning::rigid())
    );
    assert_ne!(
        rest,
        ribbon_map(&context, &skinning(400)),
        "joint 1 moves the right quad"
    );
}

/// The commands one cascade's casters record for `scene`.
fn recorded_casters(
    scene: &worldrunner_render_plan::RenderScene,
) -> super::super::cost_tests::Counted {
    let context = context();
    let device = context.device();
    let pipelines =
        crate::renderer::ScenePipelines::new(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut rig = Rig::moving(&context, &pipelines, scene, false);
    let sampling = worldrunner_render_plan::SceneShadowSampling::default();
    let live = crate::shadows::object_casters(&pipelines, settings(), sampling);
    rig.gpu.prepare_object_context(&live).unwrap();
    let matrix = map((0.0, 1.75), 2.8);
    let buffer = worldrunner_render_gpu::resource::upload_buffer(
        device,
        context.queue(),
        "counted caster frame",
        wgpu::BufferUsages::UNIFORM,
        &crate::shadows::light_frame(&rig.frame, matrix).to_bytes(),
    );
    let group = crate::draw::decals::empty_group(device, pipelines.frame_layout(), &buffer);
    let mut encoder = context.submission().begin(device).unwrap();
    rig.gpu
        .flush_instances_encoded(device, &mut encoder)
        .unwrap();
    let mut tally = super::super::cost_tests::Tally::default();
    drop(
        rig.gpu
            .draw_casters_prepared(&mut tally, &group, &live, (0, matrix)),
    );
    encoder.finish().submit().unwrap();
    tally.0
}

/// A caster's depth does not depend on a material's roughness, metalness or tint, so four
/// materials that differ in those alone bind one material group between them, after the light
/// frame's.
#[test]
fn casters_bind_one_material_group_for_materials_they_read_alike() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let counted = recorded_casters(&scenes::grid(3, 4));
    assert_eq!(counted.draws, 12);
    assert_eq!(counted.groups, 1 + 1, "{counted:?}");
}

/// Wind moves a caster's vertices and a cutout reads its texture, so materials that differ in
/// either keep a group each.
#[test]
fn casters_bind_a_material_group_for_each_material_they_read_differently() {
    use scenes::{Alpha, Spec};
    let _session = crate::test_support::SceneTestSession::acquire();
    let specs = [
        Spec::default(),
        Spec {
            wind: true,
            ..Spec::default()
        },
        Spec {
            alpha: Alpha::Cutout,
            double_sided: true,
            ..Spec::default()
        },
        Spec {
            alpha: Alpha::Cutout,
            double_sided: true,
            wind: true,
            ..Spec::default()
        },
    ];
    let counted = recorded_casters(&scenes::scene(&specs));
    assert_eq!(counted.groups, 1 + 4, "{counted:?}");
}

/// The classes the casters share a group by assume an unmasked caster binds only the material
/// block, the wind sampler, the contact heights, the impostor borders, the wind field and the
/// scene palette. A caster stage that reads another map or block of the material would make
/// materials that differ there cast differently from one group.
#[test]
fn unmasked_casters_bind_only_what_materials_share_by_class() {
    use crate::object::layout::GeometryLayout;
    use crate::pipeline::PipelineKey;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines =
        crate::renderer::ScenePipelines::new(context.device(), TextureFormat::Rgba16Float, 1)
            .unwrap();
    let sampling = worldrunner_render_plan::SceneShadowSampling::default();
    let live = crate::shadows::object_casters(&pipelines, settings(), sampling);
    live.family()
        .with_objects((context.device(), context.submission()), |runtime| {
            for key in PipelineKey::ALL.into_iter().filter(|key| !key.masked) {
                for geometry in [GeometryLayout::Static, GeometryLayout::Skinned] {
                    let program = live.program(runtime, key, geometry).unwrap();
                    let bindings: Vec<u32> = program
                        .layout()
                        .slots()
                        .iter()
                        .filter(|slot| slot.set == 1)
                        .map(|slot| slot.binding)
                        .collect();
                    assert!(
                        bindings
                            .iter()
                            .all(|binding| [0, 6, 7, 9, 10, 11].contains(binding)),
                        "{key:?} {geometry:?} binds {bindings:?}"
                    );
                }
            }
        });
}
