use crate::{abi::*, physics::*, physics_materials::*, result_code};

fn assert_stale(result: DisunityResult) {
    assert_eq!(result.code, result_code::ERROR);
    assert_eq!(result.detail, DisunityErrorReason::StaleHandle as u64);
}

#[test]
fn unloaded_and_unknown_materials_keep_typed_errors() {
    unsafe {
        let (mut old, mut asset, mut live, mut object, mut collider, mut current) =
            (0, 0, 0, 0, 0, 0);
        assert_eq!(disunity_world_create(&mut old).code, result_code::OK);
        assert_eq!(
            disunity_physics_material_create(old, &mut asset).code,
            result_code::OK
        );
        assert_eq!(disunity_world_destroy(old).code, result_code::OK);
        assert_eq!(disunity_world_create(&mut live).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(live, &mut object).code,
            result_code::OK
        );
        assert_eq!(
            disunity_physics_create(live, object, 1, &mut collider).code,
            result_code::OK
        );
        assert_eq!(
            disunity_physics_material_create(live, &mut current).code,
            result_code::OK
        );
        assert_eq!(
            disunity_shared_material_write(live, collider, current).code,
            result_code::OK
        );
        let mut value = DisunityPhysicsMaterial2d::default();
        for invalid in [asset, u64::MAX] {
            assert_stale(disunity_physics_material_read(live, invalid, &mut value));
            assert_stale(disunity_physics_material_write(live, invalid, &value));
            assert_stale(disunity_shared_material_write(live, collider, invalid));
            let mut retained = 0;
            assert_eq!(
                disunity_shared_material_read(live, collider, &mut retained).code,
                result_code::OK
            );
            assert_eq!(retained, current);
        }
        assert_eq!(disunity_world_destroy(live).code, result_code::OK);
    }
}

#[test]
fn material_abi_preserves_identity_validation_and_component_writes() {
    unsafe {
        let (mut world, mut object, mut asset, mut body, mut collider) = (0, 0, 0, 0, 0);
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
        assert_eq!(
            disunity_physics_material_create(world, &mut asset).code,
            result_code::OK
        );
        assert_eq!(
            disunity_physics_create(world, object, 0, &mut body).code,
            result_code::OK
        );
        assert_eq!(
            disunity_physics_create(world, object, 1, &mut collider).code,
            result_code::OK
        );
        validate_coefficients(world, asset);
        check_component_writes(world, asset, body, collider);
        check_foreign_handles(world, asset, collider);
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

fn validate_coefficients(world: u64, asset: u64) {
    unsafe {
        let good = DisunityPhysicsMaterial2d {
            friction: 0.6,
            bounciness: 0.8,
        };
        assert_eq!(
            disunity_physics_material_write(world, asset, &good).code,
            result_code::OK
        );
        for value in [-1.0, f32::NAN, f32::INFINITY] {
            let invalid = DisunityPhysicsMaterial2d {
                friction: value,
                ..good
            };
            assert_eq!(
                disunity_physics_material_write(world, asset, &invalid).detail,
                DisunityErrorReason::Physics as u64
            );
            let invalid = DisunityPhysicsMaterial2d {
                bounciness: value,
                ..good
            };
            assert_eq!(
                disunity_physics_material_write(world, asset, &invalid).detail,
                DisunityErrorReason::Physics as u64
            );
        }
        let mut read = DisunityPhysicsMaterial2d::default();
        assert_eq!(
            disunity_physics_material_read(world, asset, &mut read).code,
            result_code::OK
        );
        assert_eq!(
            (read.friction, read.bounciness),
            (good.friction, good.bounciness)
        );
    }
}

fn check_component_writes(world: u64, asset: u64, body: u64, collider: u64) {
    unsafe {
        for component in [body, collider] {
            assert_eq!(
                disunity_shared_material_write(world, component, asset).code,
                result_code::OK
            );
        }
        let mut b = DisunityBody2d::default();
        assert_eq!(
            disunity_body_read(world, body, &mut b).code,
            result_code::OK
        );
        b.mass = 2.0;
        assert_eq!(disunity_body_write(world, body, &b).code, result_code::OK);
        let mut c = DisunityCollider2d::default();
        assert_eq!(
            disunity_collider_read(world, collider, &mut c).code,
            result_code::OK
        );
        c.size_x = 2.0;
        assert_eq!(
            disunity_collider_write(world, collider, &c).code,
            result_code::OK
        );
        for component in [body, collider] {
            let mut handle = 0;
            assert_eq!(
                disunity_shared_material_read(world, component, &mut handle).code,
                result_code::OK
            );
            assert_eq!(handle, asset);
        }
    }
}

fn check_foreign_handles(world: u64, asset: u64, collider: u64) {
    unsafe {
        let mut foreign = 0;
        assert_eq!(disunity_world_create(&mut foreign).code, result_code::OK);
        let mut read = DisunityPhysicsMaterial2d::default();
        assert_stale(disunity_physics_material_read(foreign, asset, &mut read));
        assert_stale(disunity_physics_material_write(foreign, asset, &read));
        let mut foreign_asset = 0;
        assert_eq!(
            disunity_physics_material_create(foreign, &mut foreign_asset).code,
            result_code::OK
        );
        assert_stale(disunity_shared_material_write(
            world,
            collider,
            foreign_asset,
        ));
        assert_stale(disunity_shared_material_write(world, collider, u64::MAX));
        let mut handle = 0;
        assert_eq!(
            disunity_shared_material_read(world, collider, &mut handle).code,
            result_code::OK
        );
        assert_eq!(handle, asset);
        assert_eq!(
            disunity_shared_material_write(world, collider, 0).code,
            result_code::OK
        );
        let mut handle = asset;
        assert_eq!(
            disunity_shared_material_read(world, collider, &mut handle).code,
            result_code::OK
        );
        assert_eq!(handle, 0);
        assert_eq!(disunity_world_destroy(foreign).code, result_code::OK);
    }
}
