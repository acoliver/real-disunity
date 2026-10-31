use super::{context, fixture::view};
use crate::{compositor::FrameRenderer, frame::FrameUniform, shadows::light_frame};
use glam::{Mat4, Vec3};
use worldrunner_render_post::ProjectionJitter;

#[test]
fn shadow_caster_frames_carry_the_light_matrix_without_viewer_jitter() {
    let context = context();
    let mut renderer = FrameRenderer::new(context.device());
    renderer
        .set_projection_jitter(Some(ProjectionJitter::default()))
        .unwrap();
    let camera = renderer
        .prepare_uniform(FrameUniform::unlit(&view()))
        .unwrap();
    assert!(camera.temporal.is_some());
    let light = Mat4::from_scale(Vec3::new(0.5, 0.25, 0.125));
    let caster = light_frame(&camera, light);
    assert_eq!(caster.view_projection, light);
    assert_eq!(caster.temporal, None);
    assert_eq!(caster.selection_view_projection(), light);
    let expected: Vec<u8> = light
        .to_cols_array()
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(caster.to_bytes()[..64], expected[..]);
}

#[test]
fn shadow_caster_frames_carry_exactly_zero_material_mip_bias() {
    let mut camera = FrameUniform::unlit(&view());
    camera.material_mip_bias = -2.0;
    assert_eq!(camera.to_bytes()[316..320], (-2.0_f32).to_le_bytes());
    let caster = light_frame(&camera, Mat4::IDENTITY);
    assert_eq!(caster.material_mip_bias.to_bits(), 0.0_f32.to_bits());
    assert_eq!(caster.to_bytes()[316..320], 0.0_f32.to_le_bytes());
    let mut unbiased = camera;
    unbiased.material_mip_bias = 0.0;
    assert_eq!(
        caster.to_bytes(),
        light_frame(&unbiased, Mat4::IDENTITY).to_bytes()
    );
}
