use super::ComponentError;
use crate::scene::{
    ObjectId, Reference, Scene, Value,
    read::{RGBA, flag, integer, number, vector},
};

pub(crate) const CAMERA: i32 = 20;
const RENDER_SETTINGS: i32 = 104;

/// Unity's `CameraClearFlags`, with its serialized values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearFlags {
    Skybox = 1,
    SolidColor = 2,
    Depth = 3,
    Nothing = 4,
}

/// An orthographic camera covering the whole screen and every layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub enabled: bool,
    pub clear: ClearFlags,
    /// Linear RGBA background.
    pub background: [f64; 4],
    pub orthographic_size: f64,
    pub near: f64,
    pub far: f64,
    pub depth: f64,
}

impl Scene {
    /// A Skybox camera clears to its background only when the scene has no skybox material,
    /// so a scene that sets one fails here.
    pub fn camera(&self, id: ObjectId) -> Result<Camera, ComponentError> {
        let fields = self.expect_class(id, CAMERA, "Camera")?;
        let error = |message: &str| self.component_error(id, message);
        if !flag(fields, "orthographic", false).ok_or_else(|| error("invalid orthographic"))? {
            return Err(error("perspective cameras are not supported"));
        }
        check_whole_screen(fields).map_err(error)?;
        let clear = match integer(fields, "m_ClearFlags", 1) {
            Some(1) => ClearFlags::Skybox,
            Some(2) => ClearFlags::SolidColor,
            Some(3) => ClearFlags::Depth,
            Some(4) => ClearFlags::Nothing,
            _ => return Err(error("invalid m_ClearFlags")),
        };
        if clear == ClearFlags::Skybox && self.has_skybox_material() {
            return Err(error("the scene's skybox material is not supported"));
        }
        Ok(Camera {
            enabled: self.field(id, "m_Enabled", flag(fields, "m_Enabled", true))?,
            clear,
            background: self.field(
                id,
                "m_BackGroundColor",
                vector(fields, "m_BackGroundColor", RGBA, [0.0, 0.0, 0.0, 0.0]),
            )?,
            orthographic_size: self.field(
                id,
                "orthographic size",
                number(fields, "orthographic size", 5.0),
            )?,
            near: self.field(
                id,
                "near clip plane",
                number(fields, "near clip plane", 0.3),
            )?,
            far: self.field(
                id,
                "far clip plane",
                number(fields, "far clip plane", 1000.0),
            )?,
            depth: self.field(id, "m_Depth", number(fields, "m_Depth", 0.0))?,
        })
    }

    fn has_skybox_material(&self) -> bool {
        self.objects()
            .filter(|(_, object)| object.class_id == RENDER_SETTINGS)
            .any(|(_, object)| {
                object
                    .fields
                    .get("m_SkyboxMaterial")
                    .and_then(Value::as_reference)
                    .is_some_and(|material| material != Reference::Null)
            })
    }
}

fn check_whole_screen(fields: &Value) -> Result<(), &'static str> {
    let rect = vector(
        fields,
        "m_NormalizedViewPortRect",
        ["x", "y", "width", "height"],
        [0.0, 0.0, 1.0, 1.0],
    );
    if rect != Some([0.0, 0.0, 1.0, 1.0]) {
        return Err("only a full-screen viewport rect is supported");
    }
    let mask = fields
        .get("m_CullingMask")
        .map_or(Some(u32::MAX.into()), |mask| integer(mask, "m_Bits", 0));
    if mask != Some(u32::MAX.into()) {
        return Err("only a culling mask of every layer is supported");
    }
    if fields
        .get("m_TargetTexture")
        .and_then(Value::as_reference)
        .is_some_and(|target| target != Reference::Null)
    {
        return Err("rendering to a target texture is not supported");
    }
    Ok(())
}

/// A component that sits beside a camera and has only an enabled flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraParticipant {
    AudioListener = 81,
    GuiLayer = 92,
    FlareLayer = 124,
}

impl Scene {
    /// The kind and enabled flag of an `AudioListener`, `GUILayer` or `FlareLayer`.
    pub fn camera_participant(
        &self,
        id: ObjectId,
    ) -> Result<(CameraParticipant, bool), ComponentError> {
        let object = self
            .object(id)
            .ok_or_else(|| self.component_error(id, "is not in the scene"))?;
        let kind = match object.class_id {
            81 => CameraParticipant::AudioListener,
            92 => CameraParticipant::GuiLayer,
            124 => CameraParticipant::FlareLayer,
            _ => return Err(self.component_error(id, "is not a camera participant")),
        };
        let enabled = self.field(id, "m_Enabled", flag(&object.fields, "m_Enabled", true))?;
        Ok((kind, enabled))
    }
}
