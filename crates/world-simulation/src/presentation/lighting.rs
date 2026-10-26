use super::SpriteRenderer;
use crate::{ComponentDesc, ComponentError, World, WorldError};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum SpriteLighting {
    #[default]
    Unlit = 0,
    Global = 1,
}
impl TryFrom<u32> for SpriteLighting {
    type Error = WorldError;
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unlit),
            1 => Ok(Self::Global),
            _ => Err(unsupported("sharedMaterial")),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Light2dType {
    Parametric,
    Freeform,
    Sprite,
    Point,
    #[default]
    Global,
    Spot,
}
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalLight2d {
    pub enabled: bool,
    pub light_type: Light2dType,
    pub blend_style_index: u32,
    pub color: [f32; 4],
    pub intensity: f32,
    pub sorting_layers: Vec<i32>,
}
impl Default for GlobalLight2d {
    fn default() -> Self {
        Self {
            enabled: true,
            light_type: Light2dType::Global,
            blend_style_index: 0,
            color: [1.; 4],
            intensity: 1.,
            sorting_layers: vec![0],
        }
    }
}
impl GlobalLight2d {
    pub fn validate(&self) -> Result<(), WorldError> {
        if self.light_type != Light2dType::Global {
            return Err(unsupported("lightType"));
        }
        if self.blend_style_index != 0 {
            return Err(unsupported("blendStyleIndex"));
        }
        super::color(self.color)?;
        if !self.intensity.is_finite() || self.intensity < 0. || self.sorting_layers.len() > 32768 {
            return Err(super::invalid());
        }
        Ok(())
    }
}
pub(crate) fn unsupported(field: &'static str) -> WorldError {
    WorldError::InvalidComponent(ComponentError::UnsupportedField(field))
}
/// An extraction-local summary; no world state or cache survives the frame.
#[derive(Default)]
pub struct GlobalLighting {
    scene_lit: bool,
    layers: std::collections::BTreeMap<i32, [f32; 3]>,
}
impl GlobalLighting {
    pub fn from_world(world: &World) -> Result<Self, WorldError> {
        let mut result = Self::default();
        for (_, c) in world.components.iter(world.identity) {
            if world.lifetime.components.contains(&c.handle) || world.is_unpublished(c.object) {
                continue;
            }
            if let ComponentDesc::GlobalLight2d(ref light) = c.desc
                && light.enabled
                && world.active_in_hierarchy(c.object)?
            {
                result.scene_lit = true;
                result.add(light)?;
            }
        }
        Ok(result)
    }
    fn add(&mut self, light: &GlobalLight2d) -> Result<(), WorldError> {
        let layers: std::collections::BTreeSet<_> = light.sorting_layers.iter().copied().collect();
        for layer in layers {
            let sum = self.layers.entry(layer).or_default();
            for (i, value) in sum.iter_mut().enumerate() {
                *value += light.color[i] * light.intensity;
                if !value.is_finite() {
                    return Err(super::invalid());
                }
            }
        }
        Ok(())
    }
    pub fn color(&self, sprite: &SpriteRenderer) -> [f32; 4] {
        let mut color = sprite.color;
        if sprite.lighting == SpriteLighting::Global && self.scene_lit {
            let sum = self
                .layers
                .get(&sprite.sorting_layer)
                .copied()
                .unwrap_or([0.; 3]);
            for (i, value) in sum.into_iter().enumerate() {
                color[i] *= value;
            }
        }
        color
    }
}
pub fn global_light_color(world: &World, sprite: &SpriteRenderer) -> Result<[f32; 4], WorldError> {
    if sprite.lighting == SpriteLighting::Unlit {
        return Ok(sprite.color);
    }
    Ok(GlobalLighting::from_world(world)?.color(sprite))
}
