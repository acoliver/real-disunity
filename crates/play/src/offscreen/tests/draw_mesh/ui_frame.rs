//! The UI frame on a real adapter (lavapipe in CI): the runs of UI draws that a game's shader
//! draws are drawn by that shader into their rectangles of the layer atlas, after the work the
//! game recorded. A pixel read of the atlas sees the texels, rows bottom first.
//!
//! The shaders are the synthetic ones of the mesh draw tests: `flat` writes its material's tint
//! and `layers` blends a second pass by its alpha. Every expected texel comes from the tints, the
//! rectangles and the clear, with channel values of 0 or 1 that any encoding keeps.

use worldrunner_world_simulation::graphics::{
    GraphicsSnapshot, MaterialId, OffscreenWork, TargetRef, TextureRef,
};
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, CanvasRenderer, FontBook, RectTransform, UiGeometry, UiLimits, UiTexture,
    UiVertex,
    contribution::extract_ui,
    presentation::{UiDraw, UiDrawKind, UiLayers, UiMaterial, UiPresentation},
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ObjectHandle, ParentSpace};

use super::super::super::flush::FlushOutcome;
use super::super::super::kernels::PlayerKernels;
use super::super::super::ui_frame::{UiFrame, atlas_id};
use super::super::support::{snapshot, world};
use super::{
    BLUE, GREEN, MESH, Mat4, Meshes, RED, World, byte, draw_mesh, flat, host, identity_view,
    left_half, scene, stored,
};

/// The viewport the runs are planned in: eight texels square.
const SIDE: u32 = 8;
/// The largest side the atlas may grow to.
const LIMIT: u32 = 64;
const ZERO: [u8; 4] = [0; 4];
const READ: u64 = 7;

fn material(world: &World, id: MaterialId) -> UiMaterial {
    UiMaterial {
        id,
        generation: world.material(id).unwrap().generation,
    }
}

/// A white graphic draw of the rectangle `x, y, width, height` of the viewport, lower left origin.
fn quad(material: Option<UiMaterial>, texture: Option<UiTexture>, rect: [f32; 4]) -> UiDraw {
    let [x, y, width, height] = rect;
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [
            [x, y, 0.],
            [x + width, y, 0.],
            [x + width, y + height, 0.],
            [x, y + height, 0.],
        ],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        distance_field: None,
        corner_colors: None,
        texture,
        material,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0, 0, SIDE, SIDE],
    }
}

fn plan(draws: Vec<UiDraw>) -> (UiPresentation, UiLayers) {
    let plan = UiPresentation {
        draws,
        ..UiPresentation::default()
    };
    let layers = plan.layers([SIDE, SIDE], LIMIT);
    (plan, layers)
}

/// What a flush of the frame's work and then the UI frame left in the atlas, rows bottom first.
struct Atlas {
    texels: Vec<[u8; 4]>,
    width: u32,
    outcome: FlushOutcome,
}

impl Atlas {
    fn at(&self, column: u32, row: u32) -> [u8; 4] {
        self.texels[(row * self.width + column) as usize]
    }

    /// Whether the texel at column `x` and row `y` is `expected` for every texel of the rectangle.
    fn is(&self, [x, y, width, height]: [u32; 4], expected: [u8; 4]) -> bool {
        (y..y + height).all(|row| (x..x + width).all(|column| self.at(column, row) == expected))
    }
}

/// The snapshot of `world` with the UI frame of `plan` joined to it as the player does, and the
/// number of records the game made.
fn joined(
    world: &mut World,
    plan: &UiPresentation,
    layers: &UiLayers,
) -> (UiFrame, GraphicsSnapshot, usize) {
    let frame = UiFrame::new(plan, layers).expect("the plan has runs");
    let mut frame_snapshot = snapshot(world);
    frame_snapshot.include_materials(
        world,
        plan.draws.iter().filter_map(|d| d.material).map(|m| m.id),
    );
    let own = frame_snapshot.work.len();
    frame.join(&mut frame_snapshot);
    (frame, frame_snapshot, own)
}

/// Flushes a joined snapshot with the meshes the game stores and reads the atlas back.
fn flush_atlas(frame: &UiFrame, mut joined: GraphicsSnapshot, meshes: &Meshes) -> Atlas {
    let [width, height] = frame.atlas_size();
    joined.work.push(OffscreenWork::ReadPixels {
        request: READ,
        texture: atlas_id(),
        x: 0,
        y: 0,
        width,
        height,
    });
    let directory = tempfile::tempdir().unwrap();
    let mut host = host(PlayerKernels::new(directory.path()));
    let outcome = host.flush(&joined, &frame.assets(meshes)).unwrap();
    let answer = outcome
        .pixels
        .iter()
        .find(|answer| answer.request == READ)
        .expect("the atlas was read");
    let texels = answer
        .rgba
        .chunks_exact(4)
        .map(|texel| std::array::from_fn(|channel| byte(texel[channel])))
        .collect();
    Atlas {
        texels,
        width,
        outcome,
    }
}

fn draw(world: &mut World, plan: &UiPresentation, layers: &UiLayers, meshes: &Meshes) -> Atlas {
    let (frame, snapshot, _) = joined(world, plan, layers);
    flush_atlas(&frame, snapshot, meshes)
}

#[test]
fn a_material_draw_fills_its_run_with_the_programs_colour() {
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let (plan, layers) = plan(vec![quad(
        Some(material(&world, red)),
        None,
        [3., 2., 4., 3.],
    )]);
    assert_eq!(layers.runs[0].atlas, [0, 0, 4, 3]);

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(atlas.is([0, 0, 4, 3], RED), "the run");
    assert!(atlas.is([4, 0, 4, 8], ZERO), "right of the run");
    assert!(atlas.is([0, 3, 8, 5], ZERO), "above the run");
    assert!(
        atlas.outcome.report.failures.is_empty(),
        "{:?}",
        atlas.outcome.report.failures
    );
}

/// `count` runs that each cover the whole viewport, in the colours of `materials` in turn, with a
/// default draw between them.
fn full_screen_runs(materials: &[UiMaterial], count: usize) -> (UiPresentation, UiLayers) {
    plan(
        (0..count)
            .flat_map(|number| {
                [
                    quad(
                        Some(materials[number % materials.len()]),
                        None,
                        [0., 0., 8., 8.],
                    ),
                    quad(None, None, [0., 0., 1., 1.]),
                ]
            })
            .collect(),
    )
}

#[test]
fn more_runs_than_one_page_holds_are_all_drawn_in_their_own_places() {
    let mut world = world();
    let colours = [
        (flat(&mut world, [1.0, 0.0, 0.0, 1.0]), RED),
        (flat(&mut world, [0.0, 1.0, 0.0, 1.0]), GREEN),
        (flat(&mut world, [0.0, 0.0, 1.0, 1.0]), BLUE),
    ];
    let materials: Vec<_> = colours
        .iter()
        .map(|(id, _)| material(&world, *id))
        .collect();
    let (plan, layers) = full_screen_runs(&materials, 7);
    assert_eq!(layers.runs.len(), 7, "{:?}", layers.too_large);
    assert!(layers.too_large.is_empty());
    assert_ne!(
        layers.atlas_size,
        [SIDE, SIDE],
        "seven screens need more than one page"
    );

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    for (number, run) in layers.runs.iter().enumerate() {
        let (_, colour) = colours[number % colours.len()];
        assert!(
            atlas.is(run.atlas, colour),
            "run {number} at {:?}",
            run.atlas
        );
    }
    assert!(atlas.outcome.report.failures.is_empty());
}

#[test]
fn a_run_that_covers_the_whole_screen_is_drawn_whole() {
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let (plan, layers) = plan(vec![quad(
        Some(material(&world, red)),
        None,
        [-4., -4., 20., 20.],
    )]);
    assert_eq!(layers.runs[0].atlas, [0, 0, SIDE, SIDE]);
    assert_eq!(layers.atlas_size, [SIDE, SIDE]);

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(atlas.is([0, 0, SIDE, SIDE], RED));
}

#[test]
fn a_span_per_material_draws_each_with_its_own_shader_state() {
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    let (plan, layers) = plan(vec![
        quad(Some(material(&world, red)), None, [0., 0., 2., 2.]),
        quad(Some(material(&world, blue)), None, [2., 0., 2., 2.]),
    ]);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].spans.len(), 2);

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(atlas.is([0, 0, 2, 2], RED));
    assert!(atlas.is([2, 0, 2, 2], BLUE));
}

#[test]
fn a_run_lands_in_its_atlas_rectangle_and_not_where_it_sits_on_screen() {
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let green = flat(&mut world, [0.0, 1.0, 0.0, 1.0]);
    let (plan, layers) = plan(vec![
        quad(Some(material(&world, red)), None, [5., 5., 3., 3.]),
        quad(None, None, [0., 0., 1., 1.]),
        quad(Some(material(&world, green)), None, [0., 4., 2., 2.]),
    ]);
    assert_eq!(layers.runs.len(), 2);
    assert_eq!(layers.runs[0].atlas, [0, 0, 3, 3]);
    assert_eq!(layers.runs[1].atlas, [3, 0, 2, 2]);

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(atlas.is([0, 0, 3, 3], RED), "the first run");
    assert!(atlas.is([3, 0, 2, 2], GREEN), "the second run");
    assert!(atlas.is([5, 0, 3, 8], ZERO));
}

#[test]
fn the_ui_frame_runs_after_the_games_own_work_and_leaves_it_alone() {
    let (mut world, _drawn) = scene();
    identity_view(&mut world);
    let blue = flat(&mut world, [0.0, 0.0, 1.0, 1.0]);
    draw_mesh(&mut world, (MESH, Mat4::IDENTITY), blue, (0, -1));
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let (plan, layers) = plan(vec![quad(
        Some(material(&world, red)),
        None,
        [0., 0., 8., 8.],
    )]);
    let (frame, snapshot, own) = joined(&mut world, &plan, &layers);
    assert!(own > 0, "the game recorded work of its own");
    assert!(
        matches!(&snapshot.work[own], OffscreenWork::SetRenderTarget { color, .. }
            if *color == [TargetRef::Render(atlas_id())]),
        "the UI frame starts by binding the atlas, after the game's records"
    );

    let atlas = flush_atlas(&frame, snapshot, &stored([(MESH, left_half())]));

    assert!(atlas.is([0, 0, 8, 8], RED));
    assert!(atlas.outcome.report.failures.is_empty());
}

#[test]
fn a_draws_image_is_the_main_texture_of_its_span() {
    let mut world = world();
    let red = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let used = material(&world, red);
    let identity = (1 << 40) + 3;
    let (plan, layers) = plan(vec![
        quad(
            Some(used),
            Some(UiTexture::Image(identity)),
            [0., 0., 2., 2.],
        ),
        quad(
            Some(used),
            Some(UiTexture::Image(identity + 1)),
            [2., 0., 2., 2.],
        ),
        quad(Some(used), None, [4., 0., 2., 2.]),
    ]);
    let frame = UiFrame::new(&plan, &layers).unwrap();
    let mut joined = snapshot(&mut world);
    frame.join(&mut joined);

    let blocks: Vec<_> = joined
        .work
        .iter()
        .filter_map(|work| match work {
            OffscreenWork::DrawMesh { block, .. } => Some(block.as_ref().map(|block| {
                block
                    .textures
                    .iter()
                    .map(|(name, slot)| (name.clone(), slot.texture.clone()))
                    .collect::<Vec<_>>()
            })),
            _ => None,
        })
        .collect();

    assert_eq!(
        blocks,
        [
            Some(vec![("_MainTex".to_owned(), TextureRef::Asset(identity))]),
            Some(vec![(
                "_MainTex".to_owned(),
                TextureRef::Asset(identity + 1)
            )]),
            None,
        ]
    );
}

/// A white square of `half` units around the centre of its rectangle, as a canvas mesh.
fn square(half: f32) -> std::sync::Arc<UiGeometry> {
    let corners = [[-half, -half], [-half, half], [half, half], [half, -half]];
    let vertices = corners
        .map(|[x, y]| UiVertex {
            position: [x, y, 0.],
            uv: [0., 0.],
            color: [1.; 4],
        })
        .to_vec();
    std::sync::Arc::new(UiGeometry::new(vertices, vec![0, 1, 2, 2, 3, 0]).unwrap())
}

fn ui_element(world: &mut World, parent: ObjectHandle, parts: Vec<ComponentDesc>) {
    let object = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(
            object,
            ComponentDesc::RectTransform(RectTransform::default()),
        )
        .unwrap();
    world
        .set_parent(object, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    for part in parts {
        world.add_component(object, part).unwrap();
    }
}

fn canvas_renderer(
    geometry: std::sync::Arc<UiGeometry>,
    drawn: Option<MaterialId>,
) -> ComponentDesc {
    ComponentDesc::CanvasRenderer(CanvasRenderer {
        geometry: Some(geometry),
        material_count: u32::from(drawn.is_some()),
        world_material: drawn,
        ..CanvasRenderer::default()
    })
}

#[test]
fn an_old_world_style_silhouette_widget_draws_through_its_material() {
    // The widget's image has no sprite, a material of the game's shader and a nested canvas that
    // overrides the sorting with order 7. Plain quads sit under it and over it.
    let mut world = world();
    let silhouette = flat(&mut world, [1.0, 0.0, 0.0, 1.0]);
    let root = world.create_object(ObjectDesc::default()).unwrap();
    world
        .add_component(root, ComponentDesc::RectTransform(RectTransform::default()))
        .unwrap();
    world
        .add_component(
            root,
            ComponentDesc::Canvas(Canvas {
                mode: CanvasMode::ScreenSpaceOverlay,
                ..Canvas::default()
            }),
        )
        .unwrap();
    ui_element(&mut world, root, vec![canvas_renderer(square(4.), None)]);
    let widget = vec![
        ComponentDesc::Canvas(Canvas {
            override_sorting: true,
            sorting_order: 7,
            ..Canvas::default()
        }),
        canvas_renderer(square(2.), Some(silhouette)),
    ];
    ui_element(&mut world, root, widget);
    let over = Canvas {
        override_sorting: true,
        sorting_order: 9,
        ..Canvas::default()
    };
    ui_element(
        &mut world,
        root,
        vec![
            ComponentDesc::Canvas(over),
            canvas_renderer(square(1.), None),
        ],
    );
    let fonts = FontBook::new(UiLimits::default()).unwrap();
    let extracted = extract_ui(&world, &fonts, [SIDE, SIDE]).unwrap();
    let plan = UiPresentation::from_plan(&extracted);
    let materials: Vec<_> = plan
        .draws
        .iter()
        .map(|draw| draw.material.is_some())
        .collect();
    assert_eq!(
        materials,
        [false, true, false],
        "sorted by the nested canvas order"
    );
    let layers = plan.layers([SIDE, SIDE], LIMIT);
    assert_eq!(layers.runs.len(), 1);
    assert_eq!(layers.runs[0].bounds, [2, 2, 4, 4]);

    let overlay = worldrunner_render_scene::simulation::UiOverlay::from_presentation(
        &plan,
        &std::collections::BTreeMap::new(),
        &layers,
    )
    .unwrap();
    assert_eq!(
        overlay.quad_count(),
        3,
        "the plain draws and one quad for the run"
    );

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(
        atlas.is([0, 0, 4, 4], RED),
        "the silhouette is the program's colour"
    );
    assert!(atlas.is([4, 0, 4, 8], ZERO));
    assert!(atlas.outcome.report.failures.is_empty());
}

#[test]
fn a_shader_that_cannot_load_leaves_the_run_empty_and_counts_it() {
    let mut world = world();
    let missing = super::material(&mut world, "no-such-shader", [1.0; 4], [0.0; 4]);
    let (plan, layers) = plan(vec![quad(
        Some(material(&world, missing)),
        None,
        [0., 0., 4., 4.],
    )]);

    let atlas = draw(&mut world, &plan, &layers, &stored([]));

    assert!(
        atlas.is([0, 0, 8, 8], ZERO),
        "the clear is all the atlas holds"
    );
    let report = &atlas.outcome.report;
    assert!(
        report
            .failures
            .keys()
            .any(|name| name.starts_with("shader")),
        "{:?} {:?}",
        report.failures,
        report.skipped
    );
}
