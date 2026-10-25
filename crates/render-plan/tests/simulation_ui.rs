#![cfg(feature = "ui")]
use glam::{UVec2, Vec3};
use worldrunner_render_plan::simulation::{SpriteTextures, append_ui, extract_world};
use worldrunner_render_plan::ui::{UiGlyphQuad, UiRenderPlan, UiTextContribution};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::ui::UiLayout;
use worldrunner_world_simulation::{
    ComponentDesc, LocalTransform, ObjectDesc, World, WorldSettings,
};

fn fixture() -> (World, worldrunner_world_simulation::ObjectHandle) {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let camera = world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: Vec3::new(0., 0., -10.),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    world
        .add_component(camera, ComponentDesc::Camera(Camera::default()))
        .unwrap();
    (world, camera)
}
fn glyphs(object: worldrunner_world_simulation::ObjectHandle, viewport: [u32; 2]) -> UiRenderPlan {
    UiRenderPlan {
        layout: UiLayout {
            viewport: UVec2::from_array(viewport),
            rects: vec![],
        },
        texts: vec![UiTextContribution {
            object,
            canvas: object,
            overlay: true,
            sorting_layer: 3,
            sorting_order: 5,
            color: [0.5, 1., 1., 0.25],
            texture: Some(SceneTexture::Texels {
                format: TexelFormat::R8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 2,
                height: 1,
                mips: 1,
                bytes: vec![0, 128].into_boxed_slice(),
            }),
            quads: vec![UiGlyphQuad {
                corners: [[0., 0., 0.], [20., 0., 0.], [20., 10., 0.], [0., 10., 0.]],
                uv: [[0., 1.], [1., 1.], [1., 0.], [0., 0.]],
            }],
        }],
    }
}
#[test]
fn glyph_coverage_tint_and_overlay_pixels_survive_shared_quad_conversion() {
    let (world, object) = fixture();
    let mut plan = extract_world(&world, &SpriteTextures::default(), [80, 40]).unwrap();
    append_ui(&mut plan, glyphs(object, [80, 40])).unwrap();
    let SceneTexture::Texels { bytes, .. } = &plan.textures[&0] else {
        panic!("texels expected")
    };
    assert_eq!(&**bytes, &[255, 255, 255, 0, 255, 255, 255, 128]);
    let camera = plan.camera.unwrap();
    let corner = camera
        .world_to_screen(Vec3::from_array(plan.quads[0].corners[2]))
        .unwrap();
    assert!((corner.x - 20.).abs() < 1e-4 && (corner.y - 30.).abs() < 1e-4);
    assert_eq!(
        plan.quads[0].color.map(f32::to_bits),
        [0.5, 1., 1., 0.25].map(f32::to_bits)
    );
}
#[test]
fn resize_rejects_stale_layout_without_mutating_the_previous_plan() {
    let (world, object) = fixture();
    let mut plan = extract_world(&world, &SpriteTextures::default(), [160, 80]).unwrap();
    let previous = plan.clone();
    assert!(append_ui(&mut plan, glyphs(object, [80, 40])).is_err());
    assert_eq!(plan, previous);
}
