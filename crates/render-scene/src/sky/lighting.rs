//! Celestial contribution selections specialize the existing material pipeline.
use worldrunner_render_plan::SceneCelestialLighting;

const NAMES: [&str; 3] = [
    "CELESTIAL_DIRECT_DIFFUSE",
    "CELESTIAL_ENVIRONMENT",
    "CELESTIAL_EMISSION",
];

pub(crate) fn constants(
    options: [u32; 7],
    lighting: SceneCelestialLighting,
) -> Vec<(&'static str, f64)> {
    let mut constants = crate::renderer::specular_constants(options).to_vec();
    if lighting != SceneCelestialLighting::NEUTRAL {
        constants.extend(
            NAMES
                .into_iter()
                .zip(lighting.contributions_micro())
                .map(|(name, value)| (name, f64::from(value) / 1e6)),
        );
    }
    constants
}

pub(crate) fn source(source: String, lighting: SceneCelestialLighting) -> String {
    if lighting == SceneCelestialLighting::NEUTRAL {
        return source;
    }
    let replacements = [
        (
            "if SCENE_DIRECT_DIFFUSE != 0u { diffuse = diffuse_colour; }",
            "if SCENE_DIRECT_DIFFUSE != 0u { diffuse = diffuse_colour; } diffuse *= CELESTIAL_DIRECT_DIFFUSE;",
        ),
        (
            "let irradiance = scene_diffuse_environment(normal, frame.ambient_ground.rgb, frame.ambient_sky.rgb);",
            "let irradiance = scene_diffuse_environment(normal, frame.ambient_ground.rgb, frame.ambient_sky.rgb) * CELESTIAL_ENVIRONMENT;",
        ),
        (
            "colour += albedo_sample.rgb * emission;",
            "colour += albedo_sample.rgb * emission * CELESTIAL_EMISSION;",
        ),
    ];
    let mut source = source;
    for (old, new) in replacements {
        assert_eq!(
            source.matches(old).count(),
            1,
            "celestial lighting source contract"
        );
        source = source.replace(old, new);
    }
    for name in NAMES {
        source.push_str("\noverride ");
        source.push_str(name);
        source.push_str(": f32 = 1.0;\n");
    }
    source
}

pub(crate) fn composition_source(source: String, composition: (bool, bool, bool)) -> String {
    let (physical, separate, object_exposure) = composition;
    if separate {
        source.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
            "return vec4<f32>(colour * scene_surface_transmission(input.world_position) * albedo_sample.a, albedo_sample.a);")
    } else if physical {
        source.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
            "return vec4<f32>(colour, albedo_sample.a);")
    } else if object_exposure {
        source.replace("return vec4<f32>(apply_exposure(apply_height_fog(colour, input.world_position)), albedo_sample.a);",
            "return vec4<f32>(apply_height_fog(colour, input.world_position), albedo_sample.a);")
    } else {
        source
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neutral_source_and_constants_are_identical() {
        let original = crate::shader::ENVIRONMENT_WGSL.to_owned();
        assert_eq!(
            source(original.clone(), SceneCelestialLighting::NEUTRAL),
            original
        );
        assert_eq!(
            constants([0; 7], SceneCelestialLighting::NEUTRAL),
            crate::renderer::specular_constants([0; 7])
        );
    }
    #[test]
    fn selected_contributions_validate_as_wgsl() {
        let lighting = SceneCelestialLighting::new(500_000, 2_000_000, 3_000_000).unwrap();
        let modified = source(crate::shader::ENVIRONMENT_WGSL.to_owned(), lighting);
        let module = naga::front::wgsl::parse_str(&modified).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert_eq!(
            &constants([0; 7], lighting)[7..],
            &[(NAMES[0], 0.5), (NAMES[1], 2.0), (NAMES[2], 3.0)]
        );
    }
}
