use super::*;
use fixture::{caster_scene, object, quad};

/// Ground tiles and a dense caster grid much wider than the view, so each cascade's stable
/// square holds many casters whose shadows land outside the camera slice.
fn wide_scene() -> RenderScene {
    let ground =
        (-3..=3).flat_map(|x| (-3..=3).map(move |y| object([x * 6_000, y * 6_000, 0], 0, 0)));
    let casters = (-12..=12)
        .flat_map(|x| (-12..=12).map(move |y| object([x * 1_000, y * 1_000, 1_000], 1, 1)));
    cascades::source_with(
        &caster_scene(ground.chain(casters).collect(), quad(300), None, false),
        SceneShadowCascades::camera_splits(vec![4_000, 7_000, 11_000, 19_000], 0).unwrap(),
    )
}

fn turned_view(target: [i64; 3]) -> SceneView {
    moved_view([0, -8_000, 3_000], target)
}

fn moved_view(eye: [i64; 3], target: [i64; 3]) -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: eye,
            target_millimeters: target,
        },
        *fixture::effects(600_000).camera(),
        view().viewport(),
    )
    .unwrap()
}

const TARGETS: [[i64; 3]; 4] = [
    [0, 4_000, 0],
    [3_000, 4_000, 0],
    [-3_000, 3_000, 0],
    [0, 8_000, 500],
];

fn caster_instances(renderer: &ShadowedScene) -> usize {
    (0..4)
        .map(|index| {
            renderer
                .gpu
                .cascade_view_stats(index)
                .drawn_per_lod
                .iter()
                .sum::<usize>()
        })
        .sum()
}

/// Renders `view` and returns the image and the caster instances drawn into the cascades.
fn render(
    context: &GpuContext,
    renderer: &mut ShadowedScene,
    view: &SceneView,
    unculled: bool,
) -> (crate::offscreen::RgbaImage, usize) {
    crate::draw::UNCULLED_CASCADES.set(unculled);
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    render_target(renderer, context, view, &target).unwrap();
    crate::draw::UNCULLED_CASCADES.set(false);
    (
        target.read_rgba8(context).unwrap(),
        caster_instances(renderer),
    )
}

#[test]
fn gpu_slice_culling_keeps_cascade_images_and_drops_casters_outside_the_slice() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = wide_scene();
    let mut culled = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut unculled = ShadowedScene::new(&context, &source, settings()).unwrap();
    for target in TARGETS {
        let view = turned_view(target);
        let (image, kept) = render(&context, &mut culled, &view, false);
        let (reference, all) = render(&context, &mut unculled, &view, true);
        assert!(
            image == reference,
            "slice culling changed the image at {target:?}"
        );
        assert!(kept < all, "nothing culled at {target:?}: {kept} of {all}");
    }
}

#[test]
fn gpu_retained_cascades_refresh_when_the_camera_turns_out_of_their_slice() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = wide_scene();
    // The sun has no north component, so sliding the camera north keeps each cascade's depth
    // row and, with no projection threshold, its retained map. Both renderers make the same
    // refresh decisions, so a culled map missing casters for the moved slice shows up.
    let settings = DirectionalShadowSettings {
        refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy {
            intervals: [64; 4],
            phases: [0; 4],
            max_non_near: 3,
            light_delta_radians: 0.1,
            projection_delta_texels: 1.0e6,
        },
        ..settings()
    };
    let mut retained = ShadowedScene::new(&context, &source, settings).unwrap();
    let mut unculled = ShadowedScene::new(&context, &source, settings).unwrap();
    let mut kept = 0;
    for north in [0, 40, 400, 1_500, 4_000] {
        let view = moved_view([0, north - 8_000, 3_000], [0, north + 4_000, 0]);
        let (image, _) = render(&context, &mut retained, &view, false);
        let (reference, _) = render(&context, &mut unculled, &view, true);
        assert!(
            image == reference,
            "a retained cascade missed casters at {north} mm"
        );
        kept += unculled
            .cascade_refresh_states()
            .iter()
            .filter(|state| state.last_frame() == Some(0))
            .count();
    }
    assert!(kept > 4, "the camera motion must retain some cascades");
}

/// Streaming ridged terrain much wider than the view, with two casters near the camera and a
/// wall outside the view that shadows it.
fn terrain_source() -> RenderScene {
    let base = super::terrain::terrain_scene(
        &scene(&[[0, 4_000, 1_000], [2_000, 6_000, 1_500]], 600_000, None),
        false,
        false,
    );
    // Low ridges everywhere, and a 4 m wall beside the camera, outside the view, whose
    // shadow falls into the nearest cascade's slice.
    let heights = (0..49 * 49)
        .map(|index: i32| {
            let (east, north) = (index % 49 - 24, index / 49 - 24);
            if (5..=6).contains(&east) && (-7..=-2).contains(&north) {
                4_000
            } else {
                (east * 7 + north * 13).rem_euclid(5) * 400
            }
        })
        .collect();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 49,
        rows: 49,
        origin_east_millimeters: -24_000,
        origin_north_millimeters: -24_000,
        sample_spacing_millimeters: 1_000,
        heights_millimeters: heights,
        surface_indices: vec![0; 49 * 49],
        surfaces: base.terrain().unwrap().surfaces().to_vec(),
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(4, 256, 64, 1000).unwrap()));
    cascades::source_with(
        &base
            .with_terrain(Some(std::sync::Arc::new(terrain)))
            .unwrap(),
        SceneShadowCascades::camera_splits(vec![4_000, 7_000, 11_000, 19_000], 0).unwrap(),
    )
}

fn terrain_draws(renderer: &mut ShadowedScene) -> u64 {
    renderer
        .scene_mut()
        .statistics()
        .passes
        .shadow_cascades
        .iter()
        .map(|count| count.calls)
        .sum()
}

#[test]
fn gpu_cascades_draw_only_the_terrain_chunks_their_slice_can_sample() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = terrain_source();
    let mut culled = ShadowedScene::new(&context, &source, settings()).unwrap();
    let mut unculled = ShadowedScene::new(&context, &source, settings()).unwrap();
    for target in TARGETS {
        let view = turned_view(target);
        // Streaming refines over several frames; compare once both renderers have settled.
        for _ in 0..3 {
            render(&context, &mut culled, &view, false);
            render(&context, &mut unculled, &view, true);
        }
        let (image, _) = render(&context, &mut culled, &view, false);
        let kept = terrain_draws(&mut culled);
        let (reference, _) = render(&context, &mut unculled, &view, true);
        let all = terrain_draws(&mut unculled);
        assert!(
            image == reference,
            "terrain culling changed the image at {target:?}"
        );
        assert!(
            kept < all,
            "no terrain chunk culled at {target:?}: {kept} of {all}"
        );
    }
}
