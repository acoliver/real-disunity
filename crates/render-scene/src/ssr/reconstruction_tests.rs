use super::super::{gpu, pyramid};
use worldrunner_render_plan::{
    SceneDownsampleRounding, SceneSsaoReceiver, SceneSsr, SceneSsrPyramid,
};

fn validate(source: &str) {
    let source = format!("{}\n{source}", include_str!("../ssao/fullscreen.wgsl"));
    let module = naga::front::wgsl::parse_str(&source)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
    naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
        .expect("validated reconstruction also lowers without a GPU");
    #[cfg(target_os = "macos")]
    validate_metal(&module, &info);
}

#[cfg(target_os = "macos")]
fn validate_metal(module: &naga::Module, info: &naga::valid::ModuleInfo) {
    let options = naga::back::msl::Options {
        lang_version: (2, 4),
        ..naga::back::msl::Options::default()
    };
    let (_, translation) = naga::back::msl::write_string(
        module,
        info,
        &options,
        &naga::back::msl::PipelineOptions::default(),
    )
    .expect("reconstruction lowers to Metal without a GPU");
    for entry in translation.entry_point_names {
        entry.expect("Metal entry point has complete resource translation");
    }
}

fn variants(settings: SceneSsr, samples: u32, size: [u32; 2]) {
    let sizes = settings.radiance_extents(size);
    for water in [false, true] {
        for (ao, receiver, format) in [
            (
                false,
                SceneSsaoReceiver::Nearest,
                wgpu::TextureFormat::R32Float,
            ),
            (
                true,
                SceneSsaoReceiver::Nearest,
                wgpu::TextureFormat::R32Float,
            ),
            (
                true,
                SceneSsaoReceiver::Nearest,
                wgpu::TextureFormat::Rg16Float,
            ),
            (
                true,
                SceneSsaoReceiver::DepthAwareGather,
                wgpu::TextureFormat::Rg32Float,
            ),
        ] {
            let source =
                gpu::configured_resolve_source(sizes.len(), samples, water, ao, (receiver, format));
            let source = if pyramid::shared(&settings) {
                pyramid::resolve_source(source, &sizes, samples)
            } else {
                source
            };
            assert!(source.contains("var normals: texture_2d<f32>"));
            assert!(source.contains("if level > 0u { return reflected(level, uv, sample); }"));
            assert_eq!(source.contains("var water_depth:"), water);
            assert_eq!(source.contains("var application_ao:"), ao);
            validate(&source);
        }
    }
}

#[test]
fn multiscale_shader_variants_validate_and_lower_without_device() {
    for radiance_pyramid in [SceneSsrPyramid::SixLevels, SceneSsrPyramid::Full] {
        for radiance_rounding in [
            SceneDownsampleRounding::Ceil,
            SceneDownsampleRounding::Floor,
        ] {
            for samples in [1, 2, 4, 8, 16] {
                for resolution_scale_micro in [500_000, 1_000_000] {
                    let settings = SceneSsr {
                        radiance_pyramid,
                        radiance_rounding,
                        resolution_scale_micro,
                        ..Default::default()
                    };
                    for size in [[17, 13], [1, 7], [257, 5]] {
                        variants(settings, samples, size);
                    }
                }
            }
        }
    }
}

#[test]
fn resolve_texture_budget_includes_normals_and_ao_independently() {
    use worldrunner_render_post::FrameConfig;
    let config = FrameConfig::new([17, 13], wgpu::TextureFormat::Rgba16Float, 4);
    for (radiance_pyramid, textures) in [
        (SceneSsrPyramid::SixLevels, 16),
        (SceneSsrPyramid::Full, 11),
    ] {
        for depth_relative_ao in [false, true] {
            let settings = SceneSsr {
                radiance_pyramid,
                depth_relative_ao,
                ..Default::default()
            };
            let required = textures + u32::from(depth_relative_ao);
            let sizes = settings.radiance_extents(config.size);
            let source = gpu::configured_resolve_source(
                sizes.len(),
                config.samples,
                true,
                depth_relative_ao,
                (SceneSsaoReceiver::Nearest, wgpu::TextureFormat::R32Float),
            );
            let source = if pyramid::shared(&settings) {
                pyramid::resolve_source(source, &sizes, config.samples)
            } else {
                source
            };
            assert_eq!(
                source.matches("@binding(").count(),
                usize::try_from(required + 1).unwrap(),
            );
            let mut limits = wgpu::Limits {
                max_sampled_textures_per_shader_stage: required,
                ..Default::default()
            };
            assert!(pyramid::validate(config, &settings, &limits).is_ok());
            limits.max_sampled_textures_per_shader_stage -= 1;
            assert!(pyramid::validate(config, &settings, &limits).is_err());
        }
    }
}

#[test]
fn ssr_reconstruction_has_constant_receiver_work_bound() {
    let source = include_str!("reconstruction.wgsl");
    let level = source.split("fn reflection_level(").nth(1).unwrap();
    assert!(level.contains("if level > 0u { return reflected(level, uv, sample); }"));
    assert!(!source.contains("reflection_bin_"));
    assert!(!source.contains("reflection_leaf_weight"));
    assert!(source.contains("i < 4u"));
    assert!(source.contains("clamp(2.0 * exp2(lod), 2.0, 4.0)"));
}
