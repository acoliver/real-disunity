use super::*;

#[test]
fn triangle_copy_out_keeps_uvs_topology_clip_and_draw_order() {
    for depth_test in [false, true] {
        triangle_copy_out(depth_test);
    }
}
fn triangle_copy_out(depth_test: bool) {
    use worldrunner_world_simulation::ui::presentation::UiPresentation;
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let draw = UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test,
        triangle_uv: Some([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]),
        corners: [
            [10., 10., 0.],
            [20., 10., 0.],
            [20., 20., 0.],
            [20., 20., 0.],
        ],
        uv_rect: [0.; 4],
        color: [1.; 4],
        texture: None,
        sorting_layer: 2,
        sorting_order: 3,
        hierarchy_order: 4,
        clip_rect: [5, 6, 7, 8],
    };
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(UiPresentation {
        draws: vec![draw],
        coverage: std::collections::BTreeMap::new(),
    });
    let mut meta = DisunityUiDraw::default();
    let mut quad = DisunityQuad::default();
    assert_eq!(
        unsafe { disunity_presentation_ui_draw(world, 0, &mut meta, &mut quad) }.code,
        0
    );
    assert_eq!(meta.kind, 2 | (u32::from(depth_test) << 2));
    assert_eq!(
        [meta.triangle_u0.to_bits(), meta.triangle_v2.to_bits()],
        [0.1f32.to_bits(), 0.6f32.to_bits()]
    );
    assert_eq!(
        (quad.sorting_layer, quad.sorting_order, meta.hierarchy_order),
        (2, 3, 4)
    );
    assert_eq!(
        [meta.clip_x, meta.clip_y, meta.clip_width, meta.clip_height],
        [5, 6, 7, 8]
    );
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}

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
