//! `unity_LightData`: the factor of the main light that a draw sends to its shaders.
//!
//! Old World's shaders (`Mohawk/Terrain/TerrainShader` and `Mohawk/StandardShader`) compute the
//! main light as `_MainLightColor.rgb * (shadowAtten * unity_LightData.z * saturate(dot(N,
//! _MainLightPosition.xyz)))`, and every stage of theirs that reads `unity_LightData` reads only
//! `.z`. A draw gets `(0, 0, z, 0)` with `z` 1 when the main light affects its renderer and 0
//! when it does not.
//!
//! The main light of a view is the first directional light, in component order, whose culling
//! mask shares a layer with the view's culling mask. It affects the renderers on the layers of
//! its own culling mask. A frame with no directional light has no main light and the draws leave
//! the name to the layers below (a `--global` value, or zero). A frame that has one gives every
//! draw a value, so a view that picks no main light, or one that misses the renderer's layer,
//! draws it with `z` 0.
//!
//! Where the view comes from depends on the draw:
//!
//! * A drawing list's item takes the culling mask of the culling result the list draws from, so
//!   one renderer is lit by the main light each of its culling results picks.
//! * A renderer a camera render draws is shared by every camera of the frame that sees its layer,
//!   so it has one value: the one the cameras agree on, and 0 when they pick different results.
//!   A camera a script sets up after the frame's renderers were made does not count.
//! * A mesh, procedural or batch draw has no layer: `z` is 1 when the frame has a main light.

use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::WorldLight;
use worldrunner_world_simulation::presentation::LightSettings;

use super::builtins::{draw_layer, instanced_draw_layer};
use super::frame::Mapper;
use crate::scene_light::main_light_index;

pub const LIGHT_DATA: &str = "unity_LightData";

/// Whether bit `layer` of a layer mask is set.
fn has_layer(mask: u32, layer: u32) -> bool {
    mask.checked_shr(layer).is_some_and(|bits| bits & 1 == 1)
}

/// The main light a view with a culling mask picks, as far as a draw can tell.
#[derive(Clone, Copy, Debug)]
pub(super) struct MainLight {
    /// The frame has a directional light, so every draw gets a value.
    exists: bool,
    /// The culling mask of the light the view picks, if it picks one.
    picked: Option<u32>,
}

impl MainLight {
    /// What the view with culling mask `mask` picks among `lights`.
    pub(super) fn picked_by(lights: &[WorldLight], mask: u32) -> Self {
        let sharing: Vec<&LightSettings> = lights
            .iter()
            .map(|light| &light.settings)
            .filter(|settings| settings.culling_mask & mask != 0)
            .collect();
        Self {
            exists: main_light_index(lights.iter().map(|light| &light.settings)).is_some(),
            picked: main_light_index(sharing.iter().copied())
                .map(|index| sharing[index].culling_mask),
        }
    }

    /// Whether the main light affects a renderer on `layer`, or `None` with no main light in the
    /// frame.
    pub(super) fn affects(&self, layer: u32) -> Option<bool> {
        self.exists
            .then(|| self.picked.is_some_and(|mask| has_layer(mask, layer)))
    }
}

/// Adds `unity_LightData` to `layer` as `(0, 0, 1, 0)` when `lit` says the main light affects the
/// draw and `(0, 0, 0, 0)` when it does not. `None` adds nothing.
pub(super) fn add_light_data(layer: &mut PropertyLayer, lit: Option<bool>) {
    if let Some(lit) = lit {
        let factor = if lit { 1.0 } else { 0.0 };
        layer
            .values
            .set(LIGHT_DATA, PropertyValue::Vector([0.0, 0.0, factor, 0.0]));
    }
}

/// The values a list item carries above its renderer's own, for the renderer on `layer` under
/// the main light `main`.
pub(super) fn item_values(main: &MainLight, layer: u32) -> PropertyLayer {
    let mut values = PropertyLayer::default();
    add_light_data(&mut values, main.affects(layer));
    values
}

impl Mapper<'_> {
    /// The per-draw values of the renderer on `layer` at `local_to_world`. The renderer is shared
    /// by the frame's cameras, so its light factor is the one they agree on.
    pub(super) fn renderer_draw(&self, local_to_world: &[f32; 16], layer: u32) -> PropertyLayer {
        let mut per_draw = draw_layer(local_to_world);
        add_light_data(&mut per_draw, self.cameras_agree_on(layer));
        per_draw
    }

    /// The per-draw values of a draw with no layer at `local_to_world`.
    pub(super) fn layerless_draw(&self, local_to_world: &[f32; 16]) -> PropertyLayer {
        self.with_layerless_light(draw_layer(local_to_world))
    }

    /// The per-draw values of a batch draw, which has no layer and no matrix of its own: the
    /// shader reads each instance's matrices from the batch's buffer.
    pub(super) fn layerless_batch_draw(&self) -> PropertyLayer {
        self.with_layerless_light(instanced_draw_layer())
    }

    /// `per_draw` with the light factor of a draw with no layer.
    fn with_layerless_light(&self, mut per_draw: PropertyLayer) -> PropertyLayer {
        let any = MainLight::picked_by(&self.snapshot.lights, u32::MAX);
        add_light_data(&mut per_draw, any.exists.then_some(true));
        per_draw
    }

    /// What the cameras of the frame that see `layer` say of the main light: their common answer,
    /// 0 when they differ, and nothing when no camera sees the layer.
    fn cameras_agree_on(&self, layer: u32) -> Option<bool> {
        let says: Vec<Option<bool>> = self
            .frame
            .cameras
            .values()
            .filter(|camera| has_layer(camera.culling_mask, layer))
            .map(|camera| {
                MainLight::picked_by(&self.snapshot.lights, camera.culling_mask).affects(layer)
            })
            .collect();
        let first = says.first().copied()?;
        if says.iter().all(|other| *other == first) {
            first
        } else {
            Some(false)
        }
    }
}
