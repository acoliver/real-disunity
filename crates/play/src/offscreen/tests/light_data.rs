//! `unity_LightData`, the per-draw value the main light's factor is sent in.
//!
//! Old World's shaders multiply the main light by `unity_LightData.z`. A draw gets 1 when the main
//! light affects its renderer and 0 when it does not. The main light is the first directional
//! light, in component order, whose culling mask shares a layer with the camera's or the culling
//! result's mask, and it affects the renderers on the layers of its own culling mask.

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_render_program::unity::offscreen::frame::Record;
use worldrunner_world_simulation::ComponentDesc;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::{DrawTopology, OffscreenWork};
use worldrunner_world_simulation::presentation::{Camera, Light, LightKind, LightSettings};

use super::super::frame::{Adapted, Settings, adapt};
use super::lists::{draw, drawing, lists, placed, run, scene, shaders};
use super::support::{available, camera, material, object, render_texture, renderer, shader};
use super::support::{snapshot, world};

const LIGHT_DATA: &str = "unity_LightData";
/// The mask of a camera that draws layers 0 to 5 (bit 3 among them), 20 and 27.
const CAMERA_MASK: u32 = 0x0810_a03f;
/// A light that affects the layers 0, 1, 2, 4, 5, 6 and many above 7, but not layer 3.
const BROAD_MASK: u32 = 0x087f_ff77;
/// A light that affects only layer 24, which no camera here draws.
const SHARES_NO_LAYER: u32 = 1 << 24;

/// A light of `kind` with `culling_mask`, on its own object.
fn light(world: &mut World, kind: LightKind, culling_mask: u32) {
    let object = object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    let settings = LightSettings {
        kind,
        culling_mask,
        ..LightSettings::default()
    };
    let light = Light {
        enabled: true,
        settings,
    };
    world
        .add_component(object, ComponentDesc::Light(light))
        .unwrap();
}

/// The value of `unity_LightData` in `layer`, or `None` when it holds none.
fn light_data(layer: &PropertyLayer) -> Option<[f32; 4]> {
    match layer.values.get(LIGHT_DATA) {
        Some(PropertyValue::Vector(value)) => Some(*value),
        None => None,
        other => panic!("{LIGHT_DATA} is {other:?}, not a vector"),
    }
}

/// The light data of the frame's renderer on `layer`.
fn renderer_on(adapted: &Adapted, layer: u32) -> Option<[f32; 4]> {
    let mut found = adapted
        .frame
        .renderers
        .values()
        .filter(|renderer| renderer.layer == layer);
    let renderer = found.next().expect("a renderer on the layer");
    assert!(found.next().is_none(), "one renderer on the layer");
    light_data(&renderer.per_draw)
}

/// A world with one camera of `mask` that renders to a target, a renderer on each of layers 0 and
/// 3, and the lights `lights` give, adapted to a frame.
fn camera_frame(mask: u32, lights: &[(LightKind, u32)]) -> Adapted {
    let mut world = world();
    let target = render_texture(&mut world, [4, 4]);
    let shader = shader(&mut world, "lit");
    let lit = material(&mut world, shader);
    for (number, layer) in [(100, 0), (101, 3)] {
        renderer(&mut world, layer, number, vec![Some(lit)]);
    }
    for (kind, culling_mask) in lights {
        light(&mut world, *kind, *culling_mask);
    }
    let settings = Camera {
        culling_mask: mask,
        ..Camera::default()
    };
    let camera = camera(&mut world, settings, Vec3::ZERO, Quat::IDENTITY);
    world
        .record_offscreen(OffscreenWork::CameraRender { camera, target })
        .unwrap();
    let snapshot = snapshot(&mut world);
    adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    )
}

#[test]
fn a_camera_gives_each_renderer_the_factor_of_the_main_light_it_picks() {
    // The first light shares no layer with the camera, so the second is the main light. It
    // affects layer 0 and not layer 3, which the camera draws.
    let adapted = camera_frame(
        CAMERA_MASK,
        &[
            (LightKind::Directional, SHARES_NO_LAYER),
            (LightKind::Directional, BROAD_MASK),
        ],
    );
    assert_eq!(renderer_on(&adapted, 0), Some([0.0, 0.0, 1.0, 0.0]));
    assert_eq!(renderer_on(&adapted, 3), Some([0.0, 0.0, 0.0, 0.0]));
}

#[test]
fn a_light_that_shares_a_layer_with_the_camera_is_the_main_light_even_when_it_is_first() {
    // The first light shares layer 3 with the camera and affects only that layer, so the broad
    // light after it is not the main light.
    let adapted = camera_frame(
        CAMERA_MASK,
        &[
            (LightKind::Directional, 1 << 3),
            (LightKind::Directional, BROAD_MASK),
        ],
    );
    assert_eq!(renderer_on(&adapted, 0), Some([0.0, 0.0, 0.0, 0.0]));
    assert_eq!(renderer_on(&adapted, 3), Some([0.0, 0.0, 1.0, 0.0]));
}

#[test]
fn only_a_directional_light_is_the_main_light() {
    let adapted = camera_frame(
        CAMERA_MASK,
        &[
            (LightKind::Point, u32::MAX),
            (LightKind::Directional, BROAD_MASK),
        ],
    );
    assert_eq!(renderer_on(&adapted, 0), Some([0.0, 0.0, 1.0, 0.0]));
}

#[test]
fn a_frame_with_no_directional_light_leaves_the_light_data_missing() {
    let adapted = camera_frame(CAMERA_MASK, &[(LightKind::Point, u32::MAX)]);
    assert_eq!(renderer_on(&adapted, 0), None);
}

#[test]
fn a_directional_light_that_shares_no_layer_with_the_camera_lights_nothing_it_draws() {
    let adapted = camera_frame(CAMERA_MASK, &[(LightKind::Directional, SHARES_NO_LAYER)]);
    assert_eq!(renderer_on(&adapted, 0), Some([0.0; 4]));
    assert_eq!(renderer_on(&adapted, 3), Some([0.0; 4]));
}

/// A scene whose culling result 5 has `mask`, a renderer on each of layers 0 and 3 and the lights
/// `lights` give, with a list that draws both, adapted to a frame.
fn list_frame(mask: u32, lights: &[(LightKind, u32)]) -> Adapted {
    let mut world = world();
    let [lit, ..] = shaders(&mut world);
    for (number, layer) in [(0, 0), (1, 3)] {
        placed(&mut world, (number, layer, Vec3::ZERO), &[lit], None);
    }
    for (kind, culling_mask) in lights {
        light(&mut world, *kind, *culling_mask);
    }
    scene(&mut world, Mat4::IDENTITY, mask);
    draw(&mut world, drawing(&["UniversalForward"]));
    run(&mut world, |_| {})
}

#[test]
fn a_list_item_gets_the_factor_of_the_main_light_its_culling_result_picks() {
    let adapted = list_frame(
        CAMERA_MASK,
        &[
            (LightKind::Directional, SHARES_NO_LAYER),
            (LightKind::Directional, BROAD_MASK),
        ],
    );
    let list = lists(&adapted)[0];
    let found: Vec<_> = list
        .draws
        .iter()
        .map(|item| {
            let layer = adapted.frame.renderers[&item.renderer].layer;
            (layer, light_data(&item.per_draw))
        })
        .collect();
    assert_eq!(
        found,
        [
            (0, Some([0.0, 0.0, 1.0, 0.0])),
            (3, Some([0.0, 0.0, 0.0, 0.0]))
        ]
    );
}

#[test]
fn the_same_renderer_is_lit_by_each_culling_result_s_own_main_light() {
    // A culling result of layers 0 and 3 only picks the first light, which affects layer 3 alone.
    let lights = [
        (LightKind::Directional, 1 << 3),
        (LightKind::Directional, BROAD_MASK),
    ];
    let adapted = list_frame(0b1001, &lights);
    let item_values: Vec<_> = lists(&adapted)[0]
        .draws
        .iter()
        .map(|item| light_data(&item.per_draw))
        .collect();
    assert_eq!(
        item_values,
        [Some([0.0; 4]), Some([0.0, 0.0, 1.0, 0.0])],
        "the first light is the main light"
    );
    let broad = list_frame(1 << 0, &[(LightKind::Directional, 1 << 3), lights[1]]);
    let item_values: Vec<_> = lists(&broad)[0]
        .draws
        .iter()
        .map(|item| light_data(&item.per_draw))
        .collect();
    assert_eq!(
        item_values,
        [Some([0.0, 0.0, 1.0, 0.0])],
        "the second light is the main light"
    );
}

#[test]
fn a_list_with_no_main_light_gives_its_items_no_light_data() {
    let adapted = list_frame(CAMERA_MASK, &[(LightKind::Point, u32::MAX)]);
    assert!(
        lists(&adapted)[0]
            .draws
            .iter()
            .all(|item| light_data(&item.per_draw).is_none())
    );
}

#[test]
fn a_draw_with_no_layer_gets_the_factor_one_when_the_frame_has_a_main_light() {
    for (lights, expected) in [
        (
            vec![(LightKind::Directional, SHARES_NO_LAYER)],
            Some([0.0, 0.0, 1.0, 0.0]),
        ),
        (vec![(LightKind::Point, u32::MAX)], None),
    ] {
        let mut world = world();
        let [lit, ..] = shaders(&mut world);
        for (kind, culling_mask) in &lights {
            light(&mut world, *kind, *culling_mask);
        }
        scene(&mut world, Mat4::IDENTITY, CAMERA_MASK);
        world
            .record_offscreen(OffscreenWork::DrawProcedural {
                matrix: Mat4::IDENTITY.to_cols_array(),
                material: lit,
                pass: 0,
                topology: DrawTopology::TriangleList,
                vertex_count: 3,
                instance_count: 1,
            })
            .unwrap();
        let adapted = run(&mut world, |_| {});
        let drawn: Vec<_> = adapted
            .frame
            .records
            .iter()
            .filter_map(|record| match record {
                Record::Procedural(draw) => Some(light_data(&draw.per_draw)),
                _ => None,
            })
            .collect();
        assert_eq!(drawn, [expected], "{lights:?}");
    }
}
