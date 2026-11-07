//! What a camera render reports of how it drew: the formats of its target, the fixed-function
//! state of its draws and the constants no layer gave a value.

use worldrunner_render_program::ids::MaterialId;
use worldrunner_render_program::layers::PropertyLayer;

use super::camera::{CAMERA, LEFT, TARGET, halves};
use super::rig::Rig;

const OPAQUE: &str = "cull None, depth test LessEqual write true, color mask RGBA, blend off";

#[test]
fn a_camera_reports_its_targets_formats_and_the_state_of_the_draws_it_recorded() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, 0b110);
    let operations = rig.build(&frame).unwrap();
    let passes = operations.camera_passes();
    assert_eq!(passes.len(), 1);
    let pass = &passes[0];
    assert_eq!((pass.camera, pass.target), (CAMERA, TARGET));
    assert_eq!(pass.color, Some(wgpu::TextureFormat::Rgba8Unorm));
    assert_eq!(pass.depth, None);
    assert_eq!(pass.states, vec![(OPAQUE.to_owned(), 2)]);
    assert!(pass.without_value.is_empty(), "{:?}", pass.without_value);
}

#[test]
fn a_camera_names_the_constants_its_draws_read_with_no_value() {
    let mut rig = Rig::new();
    let mut frame = halves(&mut rig, 1 << LEFT);
    // The left renderer's material loses its tint; the right one is not drawn by this camera.
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties = PropertyLayer::default();
    let operations = rig.build(&frame).unwrap();
    let passes = operations.camera_passes();
    assert_eq!(passes[0].without_value, vec!["_Tint".to_owned()]);
}
