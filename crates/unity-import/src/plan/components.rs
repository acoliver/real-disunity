//! Engine and script components as plan records.

use crate::{
    ObjectId,
    components::{CameraParticipant, MONO_BEHAVIOUR},
    package::{PackageAsset, PackageScript},
    scene::{Reference, read::flag},
};

use super::{PlanCollider, PlanComponent, PlanError, objects::Planner, values::sprite_index};

impl Planner<'_> {
    pub(super) fn component(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let scene = self.scene;
        let object = scene
            .object(id)
            .ok_or_else(|| scene.component_error(id, "is not an object"))?;
        Ok(match object.type_name.as_str() {
            "Camera" => PlanComponent::Camera(scene.camera(id)?),
            "AudioListener" | "Behaviour" => participant(scene.camera_participant(id)?),
            "SpriteRenderer" => self.sprite_renderer(id)?,
            "Rigidbody2D" => self.body(id)?,
            name if name.ends_with("Collider2D") => self.collider(id)?,
            "Tilemap" => PlanComponent::Tilemap(self.tilemap(id)?),
            "Canvas" => self.canvas(id)?,
            _ if object.class_id == MONO_BEHAVIOUR => self.behaviour(id)?,
            other => {
                let message = format!("{other} components are not supported");
                return Err(scene.component_error(id, message).into());
            }
        })
    }

    fn sprite_renderer(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let view = self.scene.sprite_renderer(id)?;
        Ok(PlanComponent::SpriteRenderer {
            enabled: view.renderer.enabled,
            sorting_order: view.renderer.sorting_order,
            material: view.renderer.material,
            sprite: sprite_index(self, id, &view.sprite)?,
            color: view.color,
            flip_x: view.flip_x,
            flip_y: view.flip_y,
        })
    }

    fn body(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let view = self.scene.rigidbody_2d(id)?;
        Ok(PlanComponent::Rigidbody2D {
            body_type: view.body_type,
            simulated: view.simulated,
            mass: view.mass,
            gravity_scale: view.gravity_scale,
            constraints: view.constraints,
            interpolation: view.interpolation,
            material: view.material.map(|material| self.asset(material)),
        })
    }

    fn collider(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let view = self.scene.collider(id)?;
        Ok(PlanComponent::Collider2D(PlanCollider {
            enabled: view.settings.enabled,
            trigger: view.settings.trigger,
            offset: view.settings.offset,
            material: view.settings.material.map(|material| self.asset(material)),
            shape: view.shape,
        }))
    }

    fn canvas(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let view = self.scene.canvas(id)?;
        let camera = match view.camera {
            Some(camera) => Some(self.components.get(&camera).copied().ok_or_else(|| {
                self.scene
                    .component_error(id, "renders through a camera outside the plan")
            })?),
            None => None,
        };
        Ok(PlanComponent::Canvas {
            render_mode: view.render_mode,
            camera,
            plane_distance: view.plane_distance,
            pixel_perfect: view.pixel_perfect,
            sorting_layer_id: view.sorting_layer_id,
            sorting_order: view.sorting_order,
            target_display: view.target_display,
        })
    }

    /// A package component through its typed view, or a project script with its fields.
    fn behaviour(&mut self, id: ObjectId) -> Result<PlanComponent, PlanError> {
        let scene = self.scene;
        let object = scene
            .object(id)
            .ok_or_else(|| scene.component_error(id, "is not an object"))?;
        let enabled = flag(&object.fields, "m_Enabled", true)
            .ok_or_else(|| scene.component_error(id, "invalid m_Enabled"))?;
        if let Some(Reference::Package(PackageAsset::Script(script))) = scene.script(id) {
            return self.package_script(id, script, enabled);
        }
        let (script, class) = self.script_identity(id)?;
        let fields = self.fields(id, &object.fields)?;
        Ok(PlanComponent::Script {
            script,
            class,
            assembly: None,
            enabled,
            fields,
        })
    }

    fn package_script(
        &mut self,
        id: ObjectId,
        script: PackageScript,
        enabled: bool,
    ) -> Result<PlanComponent, PlanError> {
        let scene = self.scene;
        Ok(match script {
            PackageScript::Text => {
                let text = scene.text(id)?;
                self.font(text.font.font);
                PlanComponent::Text { enabled, text }
            }
            PackageScript::CanvasScaler => PlanComponent::CanvasScaler {
                enabled,
                scaler: scene.canvas_scaler(id)?,
            },
            PackageScript::GraphicRaycaster => PlanComponent::GraphicRaycaster { enabled },
            PackageScript::EventSystem => PlanComponent::EventSystem { enabled },
            PackageScript::StandaloneInputModule => {
                PlanComponent::StandaloneInputModule { enabled }
            }
            PackageScript::Light2D => PlanComponent::Light2D {
                enabled,
                light: scene.light_2d(id)?,
            },
            PackageScript::UniversalAdditionalCameraData => {
                PlanComponent::UniversalAdditionalCameraData {
                    enabled,
                    camera: scene.urp_camera(id)?,
                }
            }
            PackageScript::Tile => {
                let message = "is a Tile asset attached as a component";
                return Err(scene.component_error(id, message).into());
            }
        })
    }
}

fn participant((kind, enabled): (CameraParticipant, bool)) -> PlanComponent {
    match kind {
        CameraParticipant::AudioListener => PlanComponent::AudioListener { enabled },
        CameraParticipant::GuiLayer => PlanComponent::GuiLayer { enabled },
        CameraParticipant::FlareLayer => PlanComponent::FlareLayer { enabled },
    }
}
