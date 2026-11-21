//! Shadow queries through the ABI: which renderers cast a light's shadows and what a shadow map of
//! the light is drawn with. The call validates its pointers and forwards plain values to the
//! world simulation's shadow module, which holds the math.

use crate::{DisunityResult, errors::AbiError};
use glam::{Mat4, Vec3};
use worldrunner_world_simulation::shadows::{
    Casters, DirectionalSplit, MAX_SPLIT_PLANES, ShadowCamera, ShadowCaster, ShadowLight,
    ShadowLightKind, ShadowSplit, caster_bounds, directional_split, point_split, spot_split,
};

/// The floats a query's output buffer holds: the view, projection and culling matrices of 16 each,
/// then [`MAX_PLANES`] planes of a normal and a distance.
pub const SHADOW_OUTPUT_FLOATS: usize = 16 * 3 + MAX_PLANES * 4;
const MAX_PLANES: usize = MAX_SPLIT_PLANES;

/// What the call computes: the box around the casters, or the shadow map of a directional light's
/// split, a spot light or a face of a point light.
pub const SHADOW_QUERY_BOUNDS: u32 = 0;
pub const SHADOW_QUERY_DIRECTIONAL: u32 = 1;
pub const SHADOW_QUERY_SPOT: u32 = 2;
pub const SHADOW_QUERY_POINT: u32 = 3;

/// A shadow query. The light's and the camera's matrices travel in the call's `matrices` pointer
/// (32 floats: the light's local-to-world matrix, then the camera's projection times view
/// matrix), column-major. `light_kind` is the `LightType` number. `caster_count` is the number of
/// casters at the call's `casters` pointer. The directional fields are the split index and count,
/// the three split ratios, the shadow map resolution and the near plane offset; `face` and
/// `fov_bias` are for a point light.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityShadowRequest {
    pub query: u32,
    pub light_kind: u32,
    pub light_range: f32,
    pub light_spot_angle: f32,
    pub camera_x: f32,
    pub camera_y: f32,
    pub camera_z: f32,
    pub shadow_distance: f32,
    pub culling_mask: u32,
    pub caster_count: u32,
    pub split_index: u32,
    pub split_count: u32,
    pub ratio_x: f32,
    pub ratio_y: f32,
    pub ratio_z: f32,
    pub resolution: u32,
    pub near_plane_offset: f32,
    pub face: u32,
    pub fov_bias: f32,
}

/// A renderer a culling kept: the world-space box, the shadow casting mode (0 casts none) and the
/// layer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityShadowCaster {
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
    pub casting_mode: u32,
    pub layer: u32,
}

/// The answer to a query. `found` is 1 when casters affect the light and 0 when none does, and
/// then the rest is zero. `plane_count` planes are written to the output buffer. The box of a
/// bounds query and the culling sphere of a directional split are here; the matrices and planes
/// are in the output buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunityShadowResult {
    pub found: u32,
    pub plane_count: u32,
    pub min_x: f32,
    pub min_y: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub max_z: f32,
    pub sphere_x: f32,
    pub sphere_y: f32,
    pub sphere_z: f32,
    pub sphere_radius: f32,
}

/// What a query answers: the record and the output buffer's floats.
struct Answer {
    result: DisunityShadowResult,
    output: [f32; SHADOW_OUTPUT_FLOATS],
}

/// Answers a shadow query; see [`DisunityShadowRequest`].
///
/// # Safety
/// `request` is readable for one record; `matrices` for 32 floats; `casters` for
/// `request.caster_count` records, or null when the count is zero; `result` is writable for one
/// record; `output` is writable for [`SHADOW_OUTPUT_FLOATS`] floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_shadow_query(
    request: *const DisunityShadowRequest,
    matrices: *const f32,
    casters: *const DisunityShadowCaster,
    result: *mut DisunityShadowResult,
    output: *mut f32,
) -> DisunityResult {
    if request.is_null() || matrices.is_null() || result.is_null() || output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let request = unsafe { *request };
    let count = request.caster_count as usize;
    if count > 0 && casters.is_null() {
        return DisunityResult::invalid_argument();
    }
    let matrices = unsafe { std::slice::from_raw_parts(matrices, 32) };
    let casters: &[DisunityShadowCaster] = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(casters, count) }
    };
    match answer(&request, matrices, casters) {
        Ok(answer) => {
            unsafe {
                *result = answer.result;
                std::ptr::copy_nonoverlapping(answer.output.as_ptr(), output, SHADOW_OUTPUT_FLOATS);
            }
            DisunityResult::ok()
        }
        Err(error) => crate::abi::translate_error(None, &error),
    }
}

fn answer(
    request: &DisunityShadowRequest,
    matrices: &[f32],
    casters: &[DisunityShadowCaster],
) -> Result<Answer, AbiError> {
    let kind = ShadowLightKind::from_number(request.light_kind)
        .ok_or("this light type casts no shadow in the engine")?;
    let light = ShadowLight {
        kind,
        local_to_world: Mat4::from_cols_slice(&matrices[..16]),
        range: request.light_range,
        spot_angle: request.light_spot_angle,
    };
    let list: Vec<ShadowCaster> = casters.iter().map(caster).collect();
    let casters = Casters {
        list: &list,
        culling_mask: request.culling_mask,
    };
    let split = match request.query {
        SHADOW_QUERY_BOUNDS => return Ok(bounds(&light, &casters)),
        SHADOW_QUERY_DIRECTIONAL => {
            let camera = ShadowCamera {
                culling_matrix: Mat4::from_cols_slice(&matrices[16..32]),
                origin: Vec3::new(request.camera_x, request.camera_y, request.camera_z),
                shadow_distance: request.shadow_distance,
            };
            directional_split(&light, &camera, &casters, &directional(request))
        }
        SHADOW_QUERY_SPOT => spot_split(&light, &casters),
        SHADOW_QUERY_POINT => point_split(&light, request.face, request.fov_bias, &casters),
        _ => return Err("the shadow query is not one of the four known".into()),
    }
    .map_err(|error| AbiError::from(error.to_string()))?;
    Ok(split.map_or_else(Answer::none, Answer::of))
}

fn caster(caster: &DisunityShadowCaster) -> ShadowCaster {
    ShadowCaster {
        min: Vec3::new(caster.min_x, caster.min_y, caster.min_z),
        max: Vec3::new(caster.max_x, caster.max_y, caster.max_z),
        casting_mode: caster.casting_mode,
        layer: caster.layer,
    }
}

fn directional(request: &DisunityShadowRequest) -> DirectionalSplit {
    DirectionalSplit {
        index: request.split_index,
        count: request.split_count,
        ratio: Vec3::new(request.ratio_x, request.ratio_y, request.ratio_z),
        resolution: request.resolution,
        near_plane_offset: request.near_plane_offset,
    }
}

fn bounds(light: &ShadowLight, casters: &Casters<'_>) -> Answer {
    let mut answer = Answer::none();
    if let Some((min, max)) = caster_bounds(light, casters) {
        answer.result = DisunityShadowResult {
            found: 1,
            min_x: min.x,
            min_y: min.y,
            min_z: min.z,
            max_x: max.x,
            max_y: max.y,
            max_z: max.z,
            ..DisunityShadowResult::default()
        };
    }
    answer
}

impl Answer {
    fn none() -> Self {
        Self {
            result: DisunityShadowResult::default(),
            output: [0.0; SHADOW_OUTPUT_FLOATS],
        }
    }

    fn of(split: ShadowSplit) -> Self {
        let mut answer = Self::none();
        for (index, matrix) in [split.view, split.projection, split.culling_matrix]
            .iter()
            .enumerate()
        {
            answer.output[index * 16..index * 16 + 16].copy_from_slice(&matrix.to_cols_array());
        }
        for (index, plane) in split.planes.iter().take(MAX_PLANES).enumerate() {
            answer.output[48 + index * 4..52 + index * 4].copy_from_slice(&plane.to_array());
        }
        let sphere = split.culling_sphere;
        answer.result = DisunityShadowResult {
            found: 1,
            plane_count: split.planes.len().min(MAX_PLANES) as u32,
            sphere_x: sphere.x,
            sphere_y: sphere.y,
            sphere_z: sphere.z,
            sphere_radius: sphere.w,
            ..DisunityShadowResult::default()
        };
        answer
    }
}
