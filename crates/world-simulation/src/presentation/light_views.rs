//! The 3D lights a frame shows, for a host that lights its 3D objects.
use super::extraction::view_basis;
use super::light::LightSettings;
use crate::{ComponentDesc, ObjectHandle, World, WorldError};

/// One light a frame shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightView {
    pub object: ObjectHandle,
    /// Column-major matrix from the light's own coordinates to the frame's view basis, the basis
    /// the camera pose is in. Its third column is the light's forward axis.
    pub local_to_world: [f32; 16],
    pub settings: LightSettings,
}

/// Lists, in component order, every enabled light on an active object. A light is listed
/// whatever camera the frame presents: its own culling mask, kept in its settings, decides which
/// layers it lights.
pub fn extract_lights(world: &World) -> Result<Vec<LightView>, WorldError> {
    let basis = view_basis();
    let mut lights = Vec::new();
    for component in world.published_components() {
        let ComponentDesc::Light(light) = &component.desc else {
            continue;
        };
        if !light.enabled || !world.active_in_hierarchy(component.object)? {
            continue;
        }
        lights.push(LightView {
            object: component.object,
            local_to_world: (basis * world.presented_matrix(component.object)?).to_cols_array(),
            settings: light.settings,
        });
    }
    Ok(lights)
}
