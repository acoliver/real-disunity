use super::*;
use worldrunner_world_simulation::graphics::{ShaderSource, WorldShader};
use worldrunner_world_simulation::ui::presentation::{
    UiDraw, UiDrawKind, UiMaterial, UiPresentation,
};

fn draw(material: Option<UiMaterial>) -> UiDraw {
    UiDraw {
        kind: UiDrawKind::Graphic,
        depth_test: false,
        triangle_uv: None,
        corners: [[0.; 3]; 4],
        uv_rect: [0., 0., 1., 1.],
        color: [1.; 4],
        distance_field: None,
        corner_colors: None,
        texture: None,
        material,
        sorting_layer: 0,
        sorting_order: 0,
        hierarchy_order: 0,
        clip_rect: [0; 4],
    }
}

fn copied(world: u64) -> GraphicsSnapshot {
    let mut length = 0;
    assert_eq!(
        unsafe { disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, &mut length) }.code,
        0
    );
    let mut bytes = vec![0; length as usize];
    assert_eq!(
        unsafe { disunity_graphics_snapshot(world, bytes.as_mut_ptr(), length, &mut length) }.code,
        0
    );
    snapshot_wire::decode(&bytes).unwrap()
}

#[test]
fn the_snapshot_names_the_materials_and_shaders_of_the_ui_draws() {
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let (shader, material) = {
        let mut store = crate::lock_store();
        let session = store.session_mut(world).unwrap();
        let shader = session
            .native
            .create_shader(WorldShader {
                name: "Game/Panel".into(),
                source: Some(ShaderSource {
                    file: "panel.assets".into(),
                    path_id: 1,
                }),
            })
            .unwrap();
        let material = session.native.create_material(shader).unwrap();
        let drawn = UiMaterial {
            id: material,
            generation: 1,
        };
        session.ui_presentation = Some(UiPresentation {
            draws: vec![draw(Some(drawn)), draw(None)],
            ..UiPresentation::default()
        });
        (shader, material)
    };

    let snapshot = copied(world);

    assert!(snapshot.work.is_empty());
    assert_eq!(snapshot.shaders[&shader].name, "Game/Panel");
    assert!(snapshot.materials.contains_key(&material));
    assert!(snapshot.global.is_some());
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}

#[test]
fn a_snapshot_without_ui_draws_that_name_a_material_copies_none() {
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    crate::lock_store()
        .session_mut(world)
        .unwrap()
        .ui_presentation = Some(UiPresentation {
        draws: vec![draw(None)],
        ..UiPresentation::default()
    });

    let snapshot = copied(world);

    assert!(snapshot.materials.is_empty() && snapshot.shaders.is_empty());
    assert!(snapshot.global.is_none());
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}
