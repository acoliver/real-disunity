//! A program's common parameters: members that every variant of a program shares are stored once
//! with the program, and a variant's own list of the same constant buffer leaves them out.

use worldrunner_render_program::unity::shader::ProgramShader;
use worldrunner_render_program::unity::synthetic::shaders::camera_pass_with_common_place;

use super::camera::{LEFT, SHADER, halves, render};
use super::rig::{BLUE, CLEAR, RED, Rig, pixel};

const RIGHT: u32 = 2;

#[test]
fn a_member_only_the_programs_common_parameters_list_reaches_the_draw() {
    let mut rig = Rig::new();
    let frame = halves(&mut rig, (1 << LEFT) | (1 << RIGHT));
    let synthetic = camera_pass_with_common_place();
    rig.state.add_shader(
        SHADER,
        ProgramShader::new(synthetic.shader, synthetic.chunks).unwrap(),
    );
    let image = render(&mut rig, &frame);
    // `_Place` of each renderer puts it over its half of the target. Without the member the
    // vertex stage reads zeros, the quads collapse and the camera's clear color is all there is.
    assert_ne!(
        pixel(&image, 8, (1, 3)),
        CLEAR,
        "the left renderer is drawn"
    );
    assert_eq!(pixel(&image, 8, (1, 3)), RED);
    assert_eq!(pixel(&image, 8, (6, 3)), BLUE);
}
