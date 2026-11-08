//! Claims about the terrain programs that need no GPU.

/// Slice 1 of the program-runtime migration moves machinery only. The terrain WGSL is the
/// reference of the byte-equality captures, so a change to it is a change to what the captures
/// mean. A later modernization proposal edits the live file and the snapshot together and records
/// the decision; until then any drift fails here before it can hide inside a capture.
#[test]
fn the_live_terrain_wgsl_equals_the_frozen_snapshot() {
    let pairs = [
        (
            "shader/terrain.wgsl",
            include_str!("../shader/terrain.wgsl"),
            include_str!("fixtures/stock_terrain.wgsl"),
        ),
        (
            "terrain/pages/footprint.wgsl",
            include_str!("pages/footprint.wgsl"),
            include_str!("fixtures/footprint.wgsl"),
        ),
        (
            "terrain/pages/sampling.wgsl",
            include_str!("pages/sampling.wgsl"),
            include_str!("fixtures/sampling.wgsl"),
        ),
        (
            "terrain/pages/layered_gpu.wgsl",
            include_str!("pages/layered_gpu.wgsl"),
            include_str!("fixtures/layered_gpu.wgsl"),
        ),
    ];
    for (name, live, frozen) in pairs {
        assert!(
            live == frozen,
            "{name} differs from its frozen snapshot in terrain/fixtures"
        );
    }
}

use super::program::{Screen, Surface, material_bindings};
use crate::dynamic::capture_tests::{CONFIGS, build, context};
use worldrunner_render_program::runtime::GraphicsProgram;

const SURFACES: [Surface; 4] = [
    Surface::Plain,
    Surface::Pages,
    Surface::Layered,
    Surface::LayeredPages,
];

/// The bindings each surface source reads at set 1, written out from the shader's own
/// declarations rather than from the program's table, so a mistake in either shows.
fn expected(surface: Surface) -> Vec<u32> {
    match surface {
        Surface::Plain => vec![0, 1, 2, 3, 4, 5, 23],
        Surface::Pages | Surface::LayeredPages => vec![0, 6, 7, 23],
        Surface::Layered => [0, 1, 8, 9, 10]
            .into_iter()
            .chain(11..=17)
            .chain([21, 22, 23])
            .collect(),
    }
}

fn set_one(program: &GraphicsProgram) -> Vec<u32> {
    program
        .layout()
        .slots()
        .iter()
        .filter(|slot| slot.set == 1)
        .map(|slot| slot.binding)
        .collect()
}

#[test]
fn terrain_programs_register_and_prepare_against_the_declared_layouts() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let authority = context.submission();
    for config in &CONFIGS {
        let built = build(context.device(), config);
        let terrain = built.pipelines.terrain_context();
        assert!(terrain.on_runtime(), "{}", config.name);
        for surface in SURFACES {
            if surface.layered() && built.pipelines.layered.is_none() {
                continue;
            }
            terrain
                .family()
                .with((context.device(), authority), |runtime| {
                    let program = terrain
                        .program(runtime, surface)
                        .unwrap_or_else(|error| panic!("{} {surface:?}: {error}", config.name));
                    assert_eq!(
                        program.external_sets().first(),
                        Some(&0),
                        "the frame group is lent"
                    );
                });
        }
    }
}

#[test]
fn derived_terrain_bindings_equal_the_declared_binding_table_for_each_surface_source() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let built = build(context.device(), &CONFIGS[2]);
    let terrain = built.pipelines.terrain_context();
    for surface in SURFACES {
        if surface.layered() && built.pipelines.layered.is_none() {
            continue;
        }
        terrain
            .family()
            .with((context.device(), context.submission()), |runtime| {
                let program = terrain.program(runtime, surface).unwrap();
                let mut derived = set_one(&program);
                derived.sort_unstable();
                assert_eq!(derived, expected(surface), "{surface:?}");
                let mut declared: Vec<u32> = material_bindings(surface)
                    .iter()
                    .map(|binding| binding.at.binding)
                    .collect();
                declared.sort_unstable();
                assert_eq!(declared, expected(surface), "{surface:?} declarations");
            });
    }
}

/// The screen groups the forward contexts make are the ones the program lends, and the
/// metadata prepass's camera uniform is not one of them.
#[test]
fn screen_groups_are_recognized_from_the_terrain_source_of_their_context() {
    let ambient = crate::ssao::ambient_source(crate::shader::TERRAIN_WGSL);
    assert_eq!(Screen::of(&ambient), Some(Screen::Ambient));
    assert_eq!(Screen::of(crate::shader::TERRAIN_WGSL), None);
}

fn struct_size(source: &str, name: &str) -> u32 {
    let module = naga::front::wgsl::parse_str(source).expect("the terrain source parses");
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .expect("the terrain source has a layout");
    let (handle, _) = module
        .types
        .iter()
        .find(|(_, ty)| ty.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("the source declares {name}"));
    layouter[handle].size
}

#[test]
fn terrain_uniform_declaration_covers_the_shader_block() {
    let source = crate::shader::TERRAIN_WGSL;
    assert_eq!(
        struct_size(source, "Terrain") as usize,
        crate::terrain_uniform::TERRAIN_UNIFORM_BYTES
    );
    assert_eq!(
        struct_size(source, "Frame") as usize,
        crate::frame::FRAME_UNIFORM_BYTES
    );
}
#[test]
fn receiver_terrain_programs_load_payload_in_every_surface_and_depth_policy() {
    use crate::renderer::depth::DepthMode;
    use crate::scene_metadata::receiver::ReceiverLayout;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let layout = crate::shadows::water_layout(context.device());
    let (object, terrain) = crate::shadows::receiver_sources(true, true, true);
    let screen = crate::ssr::forward::layout(context.device());
    let built = crate::renderer::ScenePipelines::with_screen_space(
        context.device(),
        (wgpu::TextureFormat::Rgba16Float, 1),
        &object,
        &terrain,
        Some(&layout),
        Some(&screen),
    );
    for receiver in [ReceiverLayout::Front, ReceiverLayout::Opposing] {
        for depth in [DepthMode::Independent, DepthMode::Shared, DepthMode::Reuse] {
            let selected = built.for_receiver_gradients(receiver).for_depth(depth);
            assert!(selected.terrain_on_runtime());
            let terrain = selected.terrain_context();
            for surface in SURFACES {
                terrain
                    .family()
                    .with((context.device(), context.submission()), |runtime| {
                        let program = terrain.program(runtime, surface).unwrap();
                        let mut bindings: Vec<_> = program
                            .fragment()
                            .unwrap()
                            .reflected()
                            .iter()
                            .filter(|slot| slot.set == 2)
                            .map(|slot| slot.binding)
                            .collect();
                        bindings.sort_unstable();
                        assert_eq!(bindings, vec![0, 1, 2, 3, 4], "{surface:?} {depth:?}");
                        assert!(program.external_sets().contains(&2));
                    });
            }
        }
    }
}
