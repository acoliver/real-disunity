use super::*;
use worldrunner_render_plan::ui::UiDraw;

fn draw(order: i32, hierarchy: usize, clip_rect: [u32; 4]) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        corners: [[8., 8., 0.], [24., 8., 0.], [24., 24., 0.], [8., 24., 0.]],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        texture: None,
        sorting_layer: 2,
        sorting_order: order,
        hierarchy_order: hierarchy,
        clip_rect,
    }
}

#[test]
fn exported_ui_replaces_retained_contributions_when_only_order_or_clip_changes() {
    let mut player = PlayerUi::default();
    let textures = SpriteTextures::default();
    let images = PlayerTextures::default();
    let mut plan = UiPresentation {
        draws: vec![draw(-3, 2, [0, 0, 64, 64]), draw(7, 1, [12, 12, 8, 8])],
        ..Default::default()
    };
    for revision in 0..3 {
        let update = player
            .update(plan.clone(), [64, 64], &textures, &images)
            .unwrap();
        assert!(matches!(update, Some(RetainedUpdate::Ui(_))));
        assert_eq!(player.plan.as_ref(), Some(&plan));
        assert_eq!(player.updates, revision + 1);
        assert!(
            player
                .update(plan.clone(), [64, 64], &textures, &images)
                .unwrap()
                .is_none()
        );
        if revision == 0 {
            plan.draws[1].sorting_order = 9;
            plan.draws[1].hierarchy_order = 3;
        } else {
            plan.draws[1].clip_rect = [14, 14, 4, 4];
        }
    }
}
