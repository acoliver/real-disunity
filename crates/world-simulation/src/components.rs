use glam::{Mat4, Quat, Vec3};

use crate::{ComponentError, WorldError};

/// Local TRS in engine coordinates. Zero scale is valid; rotation must be unit length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for LocalTransform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl LocalTransform {
    pub(crate) fn validate(self) -> Result<(), WorldError> {
        if !self.translation.is_finite()
            || !self.scale.is_finite()
            || !self.rotation.is_finite()
            || !self.rotation.is_normalized()
        {
            return Err(WorldError::InvalidComponent(
                ComponentError::InvalidTransform,
            ));
        }
        Ok(())
    }

    pub fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Storage descriptions for the initial native component subset.
/// Physics, hierarchy and script execution are separate simulation features.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentDesc {
    Transform(LocalTransform),
    Sprite(crate::presentation::SpriteRenderer),
    GlobalLight2d(crate::presentation::GlobalLight2d),
    ShadowCaster2d,
    UniversalAdditionalCameraData(crate::presentation::UniversalAdditionalCameraData),
    AudioListener {
        enabled: bool,
    },
    GUILayer {
        enabled: bool,
    },
    FlareLayer {
        enabled: bool,
    },
    Tilemap(crate::presentation::Tilemap),
    Camera(crate::presentation::Camera),
    #[cfg(feature = "ui")]
    RectTransform(crate::ui::RectTransform),
    #[cfg(feature = "ui")]
    Canvas(crate::ui::Canvas),
    #[cfg(feature = "ui")]
    Text(crate::ui::TextRun),
    Body2d(crate::physics2d::Body2d),
    Collider2d(crate::physics2d::Collider2d),
    Renderer {
        material_slot: u32,
    },
    Script {
        assembly: String,
        type_name: String,
    },
}

impl ComponentDesc {
    pub(crate) fn validate(&self) -> Result<(), WorldError> {
        match self {
            Self::Transform(value) => value.validate(),
            Self::Sprite(value) => value.validate(),
            Self::Tilemap(value) => value.validate(),
            Self::Camera(value) => value.validate(),
            Self::GlobalLight2d(value) => value.validate(),
            Self::ShadowCaster2d => Err(WorldError::InvalidComponent(
                ComponentError::UnsupportedField("ShadowCaster2D"),
            )),
            Self::UniversalAdditionalCameraData(value) => value.validate(),
            Self::AudioListener { .. } | Self::GUILayer { .. } | Self::FlareLayer { .. } => Ok(()),
            #[cfg(feature = "ui")]
            Self::RectTransform(value) => value.validate().map_err(WorldError::from),
            #[cfg(feature = "ui")]
            Self::Canvas(value) => value.validate().map_err(WorldError::from),
            #[cfg(feature = "ui")]
            Self::Text(value) => value.validate().map_err(WorldError::from),
            Self::Body2d(value) => value.validate().map_err(WorldError::from),
            Self::Collider2d(value) => value.validate().map_err(WorldError::from),
            Self::Renderer { .. } => Ok(()),
            Self::Script {
                assembly,
                type_name,
            } => {
                if assembly.is_empty()
                    || type_name.is_empty()
                    || assembly.len() > 1024
                    || type_name.len() > 1024
                {
                    return Err(WorldError::InvalidComponent(ComponentError::InvalidScript));
                }
                Ok(())
            }
        }
    }
}
