//! The scene producer on object scenes (review 2909): an object's folded shading normal is
//! stored as its unit direction, and a frame whose Opaque pass does not reuse the prepass depth
//! stores nothing instead of refusing the frame.

use glam::{Mat3, Vec3};
use worldrunner_render_plan::{RenderScene, SceneEffects, SceneOpaquePath};
use worldrunner_render_post::FrameError;

use super::{Frame, covered, gbuffer, prepare_scene, reflections, render, scenes};
use crate::CameraPose;
use crate::device::GpuContext;
use crate::gbuffer::reference::{self, Codes, angle_degrees};

/// The draw tests' camera: two metres back and two up, looking at the origin.
const POSE: CameraPose = CameraPose {
    eye_millimeters: [0, -2_000, 2_000],
    target_millimeters: [0, 0, 0],
};

/// The four targets one observed frame of `scene` stores, with reflections requested so the
/// depth prepass runs, and the frame's scene-from-view rotation. The draw fixtures use
/// BC-compressed textures, so the context has the stock scene features.
fn observe(scene: &RenderScene) -> Result<([Vec<u8>; 4], Mat3), FrameError> {
    let effects = scene
        .effects()
        .cloned()
        .unwrap_or_else(|| {
            SceneEffects::new(crate::draw::tests::effect_parts()).expect("the fixture effects")
        })
        .with_ssr(reflections())
        .expect("the reflection request validates");
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .expect("the fixture validates with reflections");
    let uniform = scenes::frame_with(POSE, scene.effects().expect("the scene effects"));
    let context = GpuContext::shared_for_test().expect("actual GPU required");
    let configuration = Frame::Reflecting.configuration(SceneOpaquePath::GBUFFER_OBSERVATION);
    let mut drawn = prepare_scene(&context, &scene, &uniform, configuration);
    render(&context, &mut drawn, 1)?;
    let scene_from_view = Mat3::from_mat4(uniform.view).transpose();
    Ok((gbuffer(&context, &drawn.renderer), scene_from_view))
}

/// The fold blends the fixture slope's ground normal (-0.6, 0, 0.8) with the quad's +Z by the
/// fold weight, here 0.25 at a height of 500 mm, and forward lights that blend unnormalized
/// (length 0.96). The G-buffer stores its direction. Storing the raw blend decodes about 1.7
/// degrees off in this view, while the store's own quantisation stays under 0.4 degrees.
#[test]
fn the_scene_producer_stores_a_folded_object_normal_as_its_direction() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let scene = crate::draw::tests::fold_translucency::folded_scene(500);
    let (targets, scene_from_view) = observe(&scene).expect("the folded frame renders");
    let expected = Vec3::new(-0.6, 0.0, 0.8).lerp(Vec3::Z, 0.25).normalize();
    let mut objects = 0;
    for (pixel, code) in covered(&targets[3]) {
        // The quad's geometric normal is +Z; the sloped terrain's is 37 degrees from it.
        if angle_degrees(reference::g3_normal(code), Vec3::Z) > 3.0 {
            continue;
        }
        objects += 1;
        let codes = Codes::from_targets(&targets[0], &targets[1], &targets[2], &targets[3], pixel);
        let shading = reference::decode(&codes, scene_from_view).shading_normal;
        let off = angle_degrees(shading, expected);
        assert!(
            off <= 0.8,
            "{pixel}: the folded normal is off by {off} degrees"
        );
    }
    assert!(objects >= 4, "the folded quad covers {objects} texels");
}

/// A relief impostor keeps Opaque from reusing the prepass depth (it takes the separate-depth
/// fallback), so the producer has no completed depth to test against. The frame still renders,
/// and the G-buffer stays clear.
#[test]
fn a_frame_without_reused_depth_renders_and_stores_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let scene = crate::draw::tests::impostor::relief_scene();
    let (targets, _) = observe(&scene).expect("the relief frame renders");
    assert_eq!(
        covered(&targets[3]).count(),
        0,
        "the relief frame stored texels"
    );
}
