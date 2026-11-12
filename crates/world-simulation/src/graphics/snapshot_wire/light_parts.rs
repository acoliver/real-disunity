//! The encoding of the lights a snapshot copies for its cameras.
use super::WireError;
use super::codec::{Reader, Wire, Writer, wire_struct};
use crate::graphics::WorldLight;
use crate::presentation::{LightBakeType, LightKind, LightSettings, LightShadowMode};

/// Reads a `u32` and the variant of the enum it names, or the tag no variant has.
fn tag<T>(input: &mut Reader<'_>, variant: fn(u32) -> Option<T>) -> Result<T, WireError> {
    let value = u32::read(input)?;
    variant(value).ok_or(WireError::UnknownTag(value))
}

impl Wire for LightSettings {
    fn write(&self, out: &mut Writer) -> Result<(), WireError> {
        (self.kind as u32).write(out)?;
        self.color.write(out)?;
        for value in [
            self.intensity,
            self.range,
            self.spot_angle,
            self.inner_spot_angle,
        ] {
            value.write(out)?;
        }
        (self.shadows as u32).write(out)?;
        for value in [
            self.shadow_strength,
            self.shadow_bias,
            self.shadow_normal_bias,
            self.shadow_near_plane,
        ] {
            value.write(out)?;
        }
        self.culling_mask.write(out)?;
        self.rendering_layer_mask.write(out)?;
        self.bounce_intensity.write(out)?;
        self.color_temperature.write(out)?;
        self.use_color_temperature.write(out)?;
        self.cookie_size.write(out)?;
        self.cookie_texture.write(out)?;
        (self.bake_type as u32).write(out)?;
        self.shadow_resolution.write(out)
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, WireError> {
        Ok(Self {
            kind: tag(input, LightKind::from_value)?,
            color: Wire::read(input)?,
            intensity: f32::read(input)?,
            range: f32::read(input)?,
            spot_angle: f32::read(input)?,
            inner_spot_angle: f32::read(input)?,
            shadows: tag(input, LightShadowMode::from_value)?,
            shadow_strength: f32::read(input)?,
            shadow_bias: f32::read(input)?,
            shadow_normal_bias: f32::read(input)?,
            shadow_near_plane: f32::read(input)?,
            culling_mask: u32::read(input)?,
            rendering_layer_mask: u32::read(input)?,
            bounce_intensity: f32::read(input)?,
            color_temperature: f32::read(input)?,
            use_color_temperature: bool::read(input)?,
            cookie_size: f32::read(input)?,
            cookie_texture: u64::read(input)?,
            bake_type: tag(input, LightBakeType::from_value)?,
            shadow_resolution: i32::read(input)?,
        })
    }
}

wire_struct!(WorldLight {
    settings,
    world_matrix
});
