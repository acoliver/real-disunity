use worldrunner_disunity_ffi::{
    abi::*, camera_data::disunity_light2d_color, camera_layers::*, presentation::*, sprites::*,
};

#[test]
fn lit_material_crosses_the_abi_and_lighting_is_applied_once() {
    let (world, renderer, light) = fixture();
    unsafe {
        let mut value = DisunitySprite::default();
        assert_eq!(disunity_sprite_read(world, renderer, &mut value).code, 0);
        value.lighting = 77;
        assert_eq!(
            disunity_sprite_write(world, renderer, &value).detail,
            DisunityErrorReason::InvalidComponent as u64
        );
        assert_eq!(disunity_sprite_read(world, renderer, &mut value).code, 0);
        assert_eq!(value.lighting, 1);
        assert_eq!(
            disunity_light2d_color(world, light, 0.25, 0.5, 0.75, 1.).code,
            0
        );
        assert_exports(world, [0.25, 0.5, 0.75, 1.]);
        assert_eq!(
            disunity_light2d_color(world, light, 0.5, 0.25, 1., 1.).code,
            0
        );
        assert_exports(world, [0.5, 0.25, 1., 1.]);
        assert_eq!(disunity_sprite_read(world, renderer, &mut value).code, 0);
        assert_eq!(
            [value.r, value.g, value.b, value.a].map(f32::to_bits),
            [1_f32; 4].map(f32::to_bits)
        );
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
fn assert_exports(world: u64, color: [f32; 4]) {
    unsafe {
        let mut sprite = DisunitySprite::default();
        let mut matrix = DisunityMatrix::default();
        let mut count = 0;
        assert_eq!(
            disunity_world_export_sprites(world, &mut sprite, &mut matrix, 1, &mut count).code,
            0
        );
        assert_eq!(count, 1);
        assert_eq!(sprite.lighting, 1);
        assert_eq!(
            [sprite.r, sprite.g, sprite.b, sprite.a].map(f32::to_bits),
            color.map(f32::to_bits)
        );
        let mut frame = DisunityPresentation::default();
        assert_eq!(
            disunity_presentation_prepare(world, 16, 16, 8, &mut frame).code,
            0
        );
        let mut quad = DisunityQuad::default();
        assert_eq!(disunity_presentation_quad(world, 0, &mut quad).code, 0);
        assert_eq!(quad.lighting, 1);
        assert_eq!(
            [quad.r, quad.g, quad.b, quad.a].map(f32::to_bits),
            color.map(f32::to_bits)
        );
    }
}

fn fixture() -> (u64, u64, u64) {
    let (mut world, mut object, mut renderer, mut light) = (0, 0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(disunity_sprite_create(world, object, &mut renderer).code, 0);
        assert_eq!(
            disunity_camera_layer_create(world, object, 114, &mut light).code,
            0
        );
        let mut value = DisunitySprite::default();
        assert_eq!(disunity_sprite_read(world, renderer, &mut value).code, 0);
        assert_eq!(value.lighting, 0);
        value.lighting = 1;
        value.present = 1;
        value.texture = 7;
        value.width = 1;
        value.height = 1;
        value.w = 1.;
        value.h = 1.;
        value.pixels_per_unit = 1.;
        assert_eq!(disunity_sprite_write(world, renderer, &value).code, 0);
        (world, renderer, light)
    }
}
