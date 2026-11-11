//! The lights the last prepared frame shows, read from the same native image.
use crate::{DisunityHost, HostError, gpu_flush::symbol, meshes::matrix_array, native};
use worldrunner_world_simulation::presentation::{
    LightBakeType, LightKind, LightSettings, LightShadowMode,
};

/// One light the last prepared frame shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentationLight {
    /// The object's identity, 0 for an object the managed engine has not named.
    pub object: u64,
    /// Column-major matrix from the light's own coordinates to the frame's view basis. Its third
    /// column is the light's forward axis.
    pub local_to_world: [f32; 16],
    pub settings: LightSettings,
}

type Count = unsafe extern "C" fn(u64, *mut u32) -> native::DisunityResult;
type Read = unsafe extern "C" fn(
    u64,
    u32,
    *mut native::DisunityLight,
    *mut u64,
    *mut native::DisunityMatrix,
) -> native::DisunityResult;

impl DisunityHost {
    /// The lights the last call of `export_presentation` shows, in the world's order. Fails
    /// before the first frame is prepared.
    pub fn presentation_lights(&self) -> Result<Vec<PresentationLight>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let count: Count = symbol(&self._library, b"disunity_presentation_light_count\0")?;
        let read: Read = symbol(&self._library, b"disunity_presentation_light\0")?;
        let mut total = 0;
        self.check(unsafe { count(world, &mut total) })?;
        (0..total)
            .map(|index| {
                let mut record = blank_record();
                let mut object = 0;
                let mut matrix = native::DisunityMatrix::default();
                self.check(unsafe { read(world, index, &mut record, &mut object, &mut matrix) })?;
                Ok(PresentationLight {
                    object,
                    local_to_world: matrix_array(&matrix),
                    settings: settings(&record)?,
                })
            })
            .collect()
    }
}

fn blank_record() -> native::DisunityLight {
    // SAFETY: the record holds only integers and floats, for which all-zero bytes are valid.
    unsafe { std::mem::zeroed() }
}

/// The settings a light record carries. A kind, shadow mode or bake type outside its
/// enumeration is refused.
fn settings(record: &native::DisunityLight) -> Result<LightSettings, HostError> {
    let out_of_range =
        |field: &str| HostError::Managed(format!("presentation light {field} is out of range"));
    Ok(LightSettings {
        kind: LightKind::from_value(record.kind).ok_or_else(|| out_of_range("kind"))?,
        color: [record.r, record.g, record.b, record.a],
        intensity: record.intensity,
        range: record.range,
        spot_angle: record.spot_angle,
        inner_spot_angle: record.inner_spot_angle,
        shadows: LightShadowMode::from_value(record.shadows)
            .ok_or_else(|| out_of_range("shadow mode"))?,
        shadow_strength: record.shadow_strength,
        shadow_bias: record.shadow_bias,
        shadow_normal_bias: record.shadow_normal_bias,
        shadow_near_plane: record.shadow_near_plane,
        culling_mask: record.culling_mask,
        rendering_layer_mask: record.rendering_layer_mask,
        bounce_intensity: record.bounce_intensity,
        color_temperature: record.color_temperature,
        use_color_temperature: record.use_color_temperature != 0,
        cookie_size: record.cookie_size,
        cookie_texture: record.cookie_texture,
        bake_type: LightBakeType::from_value(record.lightmap_bake_type)
            .ok_or_else(|| out_of_range("bake type"))?,
        shadow_resolution: record.shadow_resolution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> native::DisunityLight {
        native::DisunityLight {
            cookie_texture: 12,
            enabled: 1,
            kind: 1,
            r: 1.0,
            g: 0.9,
            b: 0.8,
            a: 1.0,
            intensity: 1.5,
            range: 20.0,
            spot_angle: 40.0,
            inner_spot_angle: 10.0,
            shadows: 2,
            shadow_strength: 0.75,
            shadow_bias: 0.1,
            shadow_normal_bias: 0.3,
            shadow_near_plane: 0.25,
            culling_mask: 0b1011,
            rendering_layer_mask: 3,
            bounce_intensity: 0.5,
            color_temperature: 5000.0,
            use_color_temperature: 1,
            cookie_size: 4.0,
            lightmap_bake_type: 1,
            shadow_resolution: 2,
        }
    }

    #[test]
    fn a_light_record_becomes_the_settings_it_carries() {
        let expected = LightSettings {
            kind: LightKind::Directional,
            color: [1.0, 0.9, 0.8, 1.0],
            intensity: 1.5,
            range: 20.0,
            spot_angle: 40.0,
            inner_spot_angle: 10.0,
            shadows: LightShadowMode::Soft,
            shadow_strength: 0.75,
            shadow_bias: 0.1,
            shadow_normal_bias: 0.3,
            shadow_near_plane: 0.25,
            culling_mask: 0b1011,
            rendering_layer_mask: 3,
            bounce_intensity: 0.5,
            color_temperature: 5000.0,
            use_color_temperature: true,
            cookie_size: 4.0,
            cookie_texture: 12,
            bake_type: LightBakeType::Mixed,
            shadow_resolution: 2,
        };
        assert_eq!(settings(&record()).unwrap(), expected);
    }

    #[test]
    fn a_value_outside_its_enumeration_is_refused() {
        for change in [
            |r: &mut native::DisunityLight| r.kind = 8,
            |r: &mut native::DisunityLight| r.shadows = 3,
            |r: &mut native::DisunityLight| r.lightmap_bake_type = 3,
        ] {
            let mut bad = record();
            change(&mut bad);
            assert!(settings(&bad).is_err(), "{bad:?}");
        }
    }
}
