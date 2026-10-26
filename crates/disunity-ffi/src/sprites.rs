//! Scalar sprite component records delegated to native presentation state.
use crate::{
    DisunityResult,
    callbacks::mutate,
    physics::{component, create},
};
use worldrunner_world_simulation::{
    ComponentDesc,
    presentation::{Sprite, SpriteRenderer, sprite_corners},
};
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunitySprite {
    pub identity: u64,
    pub texture: u64,
    pub present: u32,
    pub width: u32,
    pub height: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub pixels_per_unit: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub enabled: u32,
    pub flip_x: u32,
    pub flip_y: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub lighting: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityBounds {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
}
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    match create(
        world,
        object,
        ComponentDesc::Sprite(SpriteRenderer::default()),
    ) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}
/// # Safety
/// Value is readable for one sprite record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_write(
    world: u64,
    handle: u64,
    value: *const DisunitySprite,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    if [v.present, v.enabled, v.flip_x, v.flip_y]
        .iter()
        .any(|&v| v > 1)
    {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::Sprite(_) = s.native.component(id)?.desc else {
            return Err(worldrunner_world_simulation::WorldError::InvalidComponent(
                worldrunner_world_simulation::ComponentError::KindMismatch,
            )
            .into());
        };
        let lighting =
            worldrunner_world_simulation::presentation::SpriteLighting::try_from(v.lighting)?;
        let sprite = (v.present == 1).then_some(Sprite {
            identity: v.identity,
            texture: v.texture,
            rect: [v.x, v.y, v.w, v.h],
            pivot: [v.pivot_x, v.pivot_y],
            pixels_per_unit: v.pixels_per_unit,
            texture_size: [v.width, v.height],
        });
        s.native
            .set_component(
                id,
                ComponentDesc::Sprite(SpriteRenderer {
                    enabled: v.enabled == 1,
                    sprite,
                    color: [v.r, v.g, v.b, v.a],
                    flip_x: v.flip_x == 1,
                    flip_y: v.flip_y == 1,
                    sorting_layer: v.sorting_layer,
                    sorting_order: v.sorting_order,
                    lighting,
                }),
            )
            .map_err(crate::errors::AbiError::from)
    })
}
/// # Safety
/// Output is writable for one sprite record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_read(
    world: u64,
    handle: u64,
    output: *mut DisunitySprite,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::Sprite(r) = &s
            .native
            .component(id)
            .map_err(crate::errors::AbiError::from)?
            .desc
        else {
            return Err("component is not a sprite renderer".into());
        };
        let v = encode(r);
        unsafe { *output = v };
        Ok(())
    })
}
/// # Safety
/// Output is writable for one bounds record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_bounds(
    world: u64,
    handle: u64,
    output: *mut DisunityBounds,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let id = component(s, handle)?;
        let component = s
            .native
            .component(id)
            .map_err(crate::errors::AbiError::from)?;
        let ComponentDesc::Sprite(r) = &component.desc else {
            return Err("component is not a sprite renderer".into());
        };
        let matrix = s
            .native
            .world_matrix(component.object)
            .map_err(crate::errors::AbiError::from)?;
        let points = r
            .sprite
            .map(|sprite| sprite_corners(sprite, matrix, r.flip_x, r.flip_y))
            .unwrap_or([matrix.w_axis.truncate().to_array(); 4]);
        let min = points
            .into_iter()
            .map(glam::Vec3::from_array)
            .reduce(glam::Vec3::min)
            .expect("four corners");
        let max = points
            .into_iter()
            .map(glam::Vec3::from_array)
            .reduce(glam::Vec3::max)
            .expect("four corners");
        // Bounds remain in the same native coordinates as managed transforms.
        unsafe {
            *output = DisunityBounds {
                min_x: min.x,
                min_y: min.y,
                min_z: min.z,
                max_x: max.x,
                max_y: max.y,
                max_z: max.z,
            }
        };
        Ok(())
    })
}

/// # Safety
/// Value is readable for one immutable content descriptor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_validate(value: *const DisunitySprite) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    let sprite = Sprite {
        identity: v.identity,
        texture: v.texture,
        rect: [v.x, v.y, v.w, v.h],
        pivot: [v.pivot_x, v.pivot_y],
        pixels_per_unit: v.pixels_per_unit,
        texture_size: [v.width, v.height],
    };
    match sprite.validate() {
        Ok(()) => DisunityResult::ok(),
        Err(e) => crate::abi::translate_error(None, &e.into()),
    }
}

pub(crate) fn encode(r: &SpriteRenderer) -> DisunitySprite {
    let mut v = DisunitySprite {
        enabled: u32::from(r.enabled),
        r: r.color[0],
        g: r.color[1],
        b: r.color[2],
        a: r.color[3],
        flip_x: u32::from(r.flip_x),
        flip_y: u32::from(r.flip_y),
        sorting_layer: r.sorting_layer,
        sorting_order: r.sorting_order,
        lighting: r.lighting as u32,
        ..Default::default()
    };
    if let Some(sprite) = r.sprite {
        v.present = 1;
        v.identity = sprite.identity;
        v.texture = sprite.texture;
        v.width = sprite.texture_size[0];
        v.height = sprite.texture_size[1];
        [v.x, v.y, v.w, v.h] = sprite.rect;
        [v.pivot_x, v.pivot_y] = sprite.pivot;
        v.pixels_per_unit = sprite.pixels_per_unit;
    }
    v
}

/// Allocates an immutable content identity after validating the descriptor.
/// # Safety
/// Value is readable and writable for one sprite record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_sprite_register(value: *mut DisunitySprite) -> DisunityResult {
    let valid = unsafe { disunity_sprite_validate(value) };
    if valid.code != 0 {
        return valid;
    }
    let identity = match worldrunner_world_simulation::presentation::allocate_sprite_identity() {
        Ok(identity) => identity,
        Err(e) => return crate::abi::translate_error(None, &e.into()),
    };
    unsafe {
        (*value).identity = identity;
    }
    DisunityResult::ok()
}

/// Creates the supported native orthographic camera with default lens and clear.
/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_create(
    world: u64,
    object: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    match create(world, object, ComponentDesc::Camera(Default::default())) {
        Ok(id) => {
            unsafe {
                *output = id;
            }
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}
