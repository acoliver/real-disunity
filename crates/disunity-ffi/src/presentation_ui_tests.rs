use super::*;

#[test]
fn ui_copy_out_requires_a_prepared_live_frame_and_valid_outputs() {
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let (mut draws, mut textures) = (99, 99);
    let mut header = crate::presentation::DisunityPresentation::default();
    let mut draw = DisunityUiDraw::default();
    let mut quad = DisunityQuad::default();
    let mut coverage = DisunityCoverage::default();
    unsafe {
        assert_ne!(
            disunity_presentation_ui_count(world, &mut draws, &mut textures).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_count(world, std::ptr::null_mut(), &mut textures).code,
            0
        );
        assert_eq!(
            crate::presentation::disunity_presentation_prepare(world, 64, 64, 8, &mut header).code,
            0
        );
        assert_eq!(
            disunity_presentation_ui_count(world, &mut draws, &mut textures).code,
            0
        );
        assert_eq!((draws, textures), (0, 0));
        assert_ne!(
            disunity_presentation_ui_draw(world, 0, &mut draw, &mut quad).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_draw(world, 0, std::ptr::null_mut(), &mut quad).code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_coverage(world, 0, &mut coverage, std::ptr::null_mut(), 0)
                .code,
            0
        );
        assert_ne!(
            disunity_presentation_ui_coverage(world, 0, &mut coverage, std::ptr::null_mut(), 1)
                .code,
            0
        );
    }
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
    assert_ne!(
        unsafe { disunity_presentation_ui_count(world, &mut draws, &mut textures) }.code,
        0
    );
}
