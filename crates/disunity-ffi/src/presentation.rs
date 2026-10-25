//! Bounded copy-out of native renderer state and its full world matrices.
use crate::{
    DisunityResult,
    callbacks::mutate,
    sprites::{DisunitySprite, encode},
};
use worldrunner_world_simulation::ComponentDesc;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityMatrix {
    pub m0: f32,
    pub m1: f32,
    pub m2: f32,
    pub m3: f32,
    pub m4: f32,
    pub m5: f32,
    pub m6: f32,
    pub m7: f32,
    pub m8: f32,
    pub m9: f32,
    pub m10: f32,
    pub m11: f32,
    pub m12: f32,
    pub m13: f32,
    pub m14: f32,
    pub m15: f32,
}
/// # Safety
/// Both output arrays are writable for capacity records; written is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_world_export_sprites(
    world: u64,
    sprites: *mut DisunitySprite,
    matrices: *mut DisunityMatrix,
    capacity: u32,
    written: *mut u32,
) -> DisunityResult {
    if written.is_null() || (capacity > 0 && (sprites.is_null() || matrices.is_null())) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let mut rows = vec![];
        for component in s.native.snapshot().components {
            let ComponentDesc::Sprite(renderer) = component.desc else {
                continue;
            };
            if !s
                .native
                .active_in_hierarchy(component.object)
                .map_err(crate::errors::AbiError::from)?
            {
                continue;
            }
            let matrix = s
                .native
                .world_matrix(component.object)
                .map_err(crate::errors::AbiError::from)?
                .to_cols_array();
            rows.push((encode(&renderer), matrix));
        }
        unsafe { *written = u32::try_from(rows.len()).map_err(|_| "sprite count overflow")? };
        if capacity == 0 {
            return Ok(());
        }
        if (capacity as usize) < rows.len() {
            return Err("sprite copy capacity is too small".into());
        }
        for (index, (sprite, matrix)) in rows.into_iter().enumerate() {
            let row = DisunityMatrix {
                m0: matrix[0],
                m1: matrix[1],
                m2: matrix[2],
                m3: matrix[3],
                m4: matrix[4],
                m5: matrix[5],
                m6: matrix[6],
                m7: matrix[7],
                m8: matrix[8],
                m9: matrix[9],
                m10: matrix[10],
                m11: matrix[11],
                m12: matrix[12],
                m13: matrix[13],
                m14: matrix[14],
                m15: matrix[15],
            };
            unsafe {
                sprites.add(index).write(sprite);
                matrices.add(index).write(row);
            }
        }
        Ok(())
    })
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityPresentation {
    pub present: u32,
    pub quads: u32,
    pub textures: u32,
    pub width: u32,
    pub height: u32,
    pub eye_x: f64,
    pub eye_y: f64,
    pub eye_z: f64,
    pub forward_x: f64,
    pub forward_y: f64,
    pub forward_z: f64,
    pub up_x: f64,
    pub up_y: f64,
    pub up_z: f64,
    pub half_height: f64,
    pub near: f64,
    pub far: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityQuad {
    pub texture: u64,
    pub textured: u32,
    pub overlay: u32,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub p0_x: f32,
    pub p0_y: f32,
    pub p0_z: f32,
    pub p1_x: f32,
    pub p1_y: f32,
    pub p1_z: f32,
    pub p2_x: f32,
    pub p2_y: f32,
    pub p2_z: f32,
    pub p3_x: f32,
    pub p3_y: f32,
    pub p3_z: f32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCoverage {
    pub identity: u64,
    pub width: u32,
    pub height: u32,
    pub length: u32,
}

/// Extracts one immutable frame from the loaded world's own layout and FontBook.
/// The next prepare or world unload releases the preceding frame.
/// # Safety
/// Output is writable for one presentation header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_prepare(
    world: u64,
    width: u32,
    height: u32,
    next_texture: u64,
    output: *mut DisunityPresentation,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        use worldrunner_world_simulation::{
            presentation::extraction::{SpriteTextures, append_ui_from, extract_world},
            ui::contribution::extract_ui,
        };

        let mut plan = extract_world(&s.native, &SpriteTextures::default(), [width, height])
            .map_err(crate::errors::AbiError::from)?;
        let ui = extract_ui(&s.native, &s.fonts, [width, height])
            .map_err(crate::errors::AbiError::from)?;
        let next = plan
            .quads
            .iter()
            .filter_map(|q| q.texture)
            .max()
            .map_or(Some(next_texture), |n| {
                n.checked_add(1).map(|n| n.max(next_texture))
            })
            .ok_or("texture identity exhausted")?;
        append_ui_from(&mut plan, ui, next)
            .map_err(|e| crate::errors::AbiError::from(e.to_string()))?;
        let mut header = DisunityPresentation {
            width,
            height,
            quads: u32::try_from(plan.quads.len()).map_err(|_| "quad count overflow")?,
            textures: u32::try_from(plan.textures.len()).map_err(|_| "texture count overflow")?,
            r: plan.clear[0],
            g: plan.clear[1],
            b: plan.clear[2],
            a: plan.clear[3],
            ..Default::default()
        };
        if let Some(state) = plan.camera_state {
            header.present = 1;
            [header.eye_x, header.eye_y, header.eye_z] = state.pose.eye_meters.to_array();
            [header.forward_x, header.forward_y, header.forward_z] = state.pose.forward.to_array();
            [header.up_x, header.up_y, header.up_z] = state.pose.up.to_array();
            let worldrunner_camera::CameraLens::Orthographic {
                half_height_meters,
                near_meters,
                far_meters,
            } = state.lens
            else {
                return Err("unsupported presentation camera lens".into());
            };
            header.half_height = half_height_meters;
            header.near = near_meters;
            header.far = far_meters;
        }
        s.presentation = Some(plan);
        unsafe {
            *output = header;
        }
        Ok(())
    })
}
/// # Safety
/// Output is writable for one quad. Index addresses the last prepared frame.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_quad(
    world: u64,
    index: u32,
    output: *mut DisunityQuad,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let q = s
            .presentation
            .as_ref()
            .and_then(|p| p.quads.get(index as usize))
            .ok_or("presentation quad index out of range")?;
        let value = DisunityQuad {
            texture: q.texture.unwrap_or(0),
            textured: u32::from(q.texture.is_some()),
            overlay: u32::from(q.overlay),
            sorting_layer: q.sorting_layer,
            sorting_order: q.sorting_order,
            p0_x: q.corners[0][0],
            p0_y: q.corners[0][1],
            p0_z: q.corners[0][2],
            p1_x: q.corners[1][0],
            p1_y: q.corners[1][1],
            p1_z: q.corners[1][2],
            p2_x: q.corners[2][0],
            p2_y: q.corners[2][1],
            p2_z: q.corners[2][2],
            p3_x: q.corners[3][0],
            p3_y: q.corners[3][1],
            p3_z: q.corners[3][2],
            u0: q.uv_rect[0],
            v0: q.uv_rect[1],
            u1: q.uv_rect[2],
            v1: q.uv_rect[3],
            r: q.color[0],
            g: q.color[1],
            b: q.color[2],
            a: q.color[3],
        };
        unsafe {
            *output = value;
        }
        Ok(())
    })
}
/// # Safety
/// Metadata is writable; bytes is writable for capacity, or null when capacity is zero.
/// A zero-capacity query reports the exact required byte length without copying.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_coverage(
    world: u64,
    index: u32,
    output: *mut DisunityCoverage,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    if output.is_null() || (capacity != 0 && bytes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (identity, texture) = s
            .presentation
            .as_ref()
            .and_then(|p| p.textures.iter().nth(index as usize))
            .ok_or("presentation texture index out of range")?;
        let width = texture.width;
        let height = texture.height;
        let data = &texture.bytes;
        let length = u32::try_from(data.len()).map_err(|_| "coverage length overflow")?;
        unsafe {
            *output = DisunityCoverage {
                identity: *identity,
                width,
                height,
                length,
            };
        }
        if capacity == 0 {
            return Ok(());
        }
        if capacity < length {
            return Err("coverage copy capacity is too small".into());
        }
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), bytes, data.len());
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "presentation_tests.rs"]
mod tests;
