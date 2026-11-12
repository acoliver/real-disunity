use super::lighting::unsupported;
use crate::WorldError;
/// Settings accepted by the current single-camera 2D pipeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UniversalAdditionalCameraData {
    pub enabled: bool,
    settings: [u32; 12],
}
impl Default for UniversalAdditionalCameraData {
    fn default() -> Self {
        Self {
            enabled: true,
            settings: [0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 1, 1],
        }
    }
}
const FIELDS: [&str; 12] = [
    "renderPostProcessing",
    "antialiasing",
    "requiresDepthTexture",
    "requiresColorTexture",
    "renderType",
    "renderShadows",
    "stopNaN",
    "dithering",
    "requiresDepthOption",
    "requiresColorOption",
    "volumeLayerMask",
    "allowXRRendering",
];
impl UniversalAdditionalCameraData {
    pub fn get(&self, property: u32) -> Option<u32> {
        self.settings.get(property as usize).copied()
    }
    pub fn set(&mut self, property: u32, value: u32) -> Result<(), WorldError> {
        let Some(field) = FIELDS.get(property as usize) else {
            return Err(unsupported("property"));
        };
        let supported = match property {
            8 | 9 => value == 0 || value == 2,
            10 => true,
            5 | 11 => value <= 1,
            _ => value == 0,
        };
        if !supported {
            return Err(unsupported(field));
        }
        self.settings[property as usize] = value;
        if property == 2 || property == 3 {
            self.settings[property as usize + 6] = 0;
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), WorldError> {
        Ok(())
    }
}

/// The renderShadows setting of the camera a frame presents: none when the frame presents no
/// camera or its camera has no camera data.
///
/// # Errors
/// Fails when the world cannot resolve the presented camera's object.
pub fn presented_render_shadows(world: &crate::World) -> Result<Option<bool>, WorldError> {
    let Some((object, _)) =
        super::extraction::selected_camera(world, world.published_components())?
    else {
        return Ok(None);
    };
    Ok(world
        .published_components()
        .find_map(|component| match &component.desc {
            crate::ComponentDesc::UniversalAdditionalCameraData(data)
                if component.object == object =>
            {
                data.get(RENDER_SHADOWS)
            }
            _ => None,
        })
        .map(|value| value != 0))
}

/// The index of renderShadows among the camera data's settings.
const RENDER_SHADOWS: u32 = 5;
