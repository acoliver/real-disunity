//! Enumerate the final modules and active vertex entries of every stock geometry consumer.
use super::{ENVIRONMENT_WGSL, TERRAIN_WGSL};

fn check(source: &str, entries: &[&str], expected: bool) {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    for name in entries {
        let entry = module
            .entry_points
            .iter()
            .find(|entry| entry.stage == naga::ShaderStage::Vertex && entry.name == *name)
            .unwrap_or_else(|| panic!("missing generated vertex entry {name}"));
        let result = entry.function.result.as_ref().unwrap();
        let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
            panic!("stock geometry outputs are structs");
        };
        let invariant = members
            .iter()
            .find_map(|member| match member.binding {
                Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { invariant })) => {
                    Some(invariant)
                }
                _ => None,
            })
            .expect("position output");
        assert!(
            invariant == expected,
            "{name}: position qualifier must match the pairing contract"
        );
    }
}

fn family((object, terrain): (String, String), paired: bool) {
    use worldrunner_render_plan::SceneSsaoReceiver::{DepthAwareGather, Nearest};
    let receivers: &[_] = if object.contains("fn screen_ao(") {
        &[Nearest, DepthAwareGather]
    } else {
        &[Nearest]
    };
    for &receiver in receivers {
        let object = crate::ssao::receiver::source(&object, receiver);
        let terrain = crate::ssao::receiver::source(&terrain, receiver);
        check(&object, &["vs_main"], paired);
        check(
            &crate::renderer::unlit_source(&object),
            &["vs_unlit"],
            false,
        );
        for source in crate::renderer::terrain_test_sources(&terrain) {
            check(&source, &["vs_terrain"], paired);
        }
    }
}

fn forward_families() {
    family((ENVIRONMENT_WGSL.to_owned(), TERRAIN_WGSL.to_owned()), true);
    for velocity in [false, true] {
        let stock = if velocity {
            (
                crate::velocity::object_source(ENVIRONMENT_WGSL),
                crate::velocity::terrain_source(TERRAIN_WGSL),
            )
        } else {
            (ENVIRONMENT_WGSL.to_owned(), TERRAIN_WGSL.to_owned())
        };
        family(stock.clone(), true);
        family(
            (
                crate::ssao::ambient_source(&stock.0),
                crate::ssao::ambient_source(&stock.1),
            ),
            true,
        );
        for ssao in [false, true] {
            for reflection in [false, true] {
                family(
                    crate::shadows::receiver_sources(ssao, reflection, velocity),
                    true,
                );
            }
        }
        for shadowed in [false, true] {
            for application_ao in [false, true] {
                family(
                    crate::ssr::forward::velocity_sources(
                        crate::shadows::capture_sources(shadowed),
                        velocity,
                        application_ao,
                    ),
                    true,
                );
            }
        }
    }
}

fn independent_families() {
    use crate::compositor::terms::{CaptureTerm, term_sources};
    let source = crate::shadows::depth_source();
    check(&source, &["vs_shadow", "vs_terrain_depth"], false);
    for depth in [false, true] {
        check(
            &crate::renderer::overlay::source(depth),
            &["vs_ui_overlay"],
            false,
        );
    }
    for physical in [false, true] {
        for separate in [false, true] {
            for exposure in [false, true] {
                for anchored in [false, true] {
                    for lighting in [
                        worldrunner_render_plan::SceneCelestialLighting::NEUTRAL,
                        worldrunner_render_plan::SceneCelestialLighting::new(
                            500_000, 2_000_000, 3_000_000,
                        )
                        .unwrap(),
                    ] {
                        let source = crate::renderer::celestial_source(
                            (physical, separate, exposure),
                            lighting,
                            anchored,
                        );
                        check(&source, &["vs_celestial"], false);
                    }
                }
            }
        }
    }
    for shadowed in [false, true] {
        for term in [
            CaptureTerm::Albedo,
            CaptureTerm::SunVisibility,
            CaptureTerm::DirectDiffuse,
            CaptureTerm::AmbientDiffuse,
            CaptureTerm::EnvironmentSpecular,
            CaptureTerm::SunSpecular,
        ] {
            let stock = term_sources(crate::shadows::capture_sources(shadowed), term);
            for (screen, reflection) in [(false, false), (true, false), (true, true)] {
                family(
                    crate::compositor::terms::sources(stock.clone(), screen, reflection),
                    false,
                );
            }
        }
    }
}

#[test]
fn generated_pipeline_position_qualifiers_match_pairing() {
    forward_families();
    for output in [
        crate::scene_metadata::MetadataOutput::Normal,
        crate::scene_metadata::MetadataOutput::Merged,
        crate::scene_metadata::MetadataOutput::Material,
    ] {
        family(crate::scene_metadata::sources(output), true);
    }
    independent_families();
}

#[test]
fn live_unlit_program_sources_are_non_invariant() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let format = wgpu::TextureFormat::Rgba16Float;
    for pipelines in [
        crate::renderer::ScenePipelines::new(context.device(), format),
        crate::renderer::ScenePipelines::with_taa_velocity(context.device(), format, 1).unwrap(),
        crate::renderer::ScenePipelines::with_ssao(context.device(), format, 1).unwrap(),
        crate::renderer::ScenePipelines::with_ssao_taa_velocity(context.device(), format, 1)
            .unwrap(),
    ] {
        check(pipelines.unlit_source(), &["vs_unlit"], false);
    }
}
