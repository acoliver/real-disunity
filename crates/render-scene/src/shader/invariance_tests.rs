//! Cross-pipeline position invariance is required before paired depth can be consumed.
use super::{ENVIRONMENT_WGSL, TERRAIN_WGSL};

fn assert_positions(source: &str, invariant: bool) {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    for entry in module
        .entry_points
        .iter()
        .filter(|entry| entry.stage == naga::ShaderStage::Vertex)
    {
        let result = entry.function.result.as_ref().unwrap();
        let naga::TypeInner::Struct { members, .. } = &module.types[result.ty].inner else {
            panic!("stock vertex outputs are structs");
        };
        assert!(
            members.iter().any(|member| {
                member.binding
                    == Some(naga::Binding::BuiltIn(naga::BuiltIn::Position {
                        invariant,
                    }))
            }),
            "{} position invariant must be {invariant}",
            entry.name
        );
    }
}

fn assert_family((object, terrain): (String, String)) {
    use worldrunner_render_plan::SceneSsaoReceiver::{DepthAwareGather, Nearest};
    let receivers: &[_] = if object.contains("fn screen_ao(") {
        &[Nearest, DepthAwareGather]
    } else {
        &[Nearest]
    };
    for &receiver in receivers {
        assert_positions(&crate::ssao::receiver::source(&object, receiver), true);
        let terrain = crate::ssao::receiver::source(&terrain, receiver);
        for source in crate::renderer::terrain_test_sources(&terrain) {
            assert_positions(&source, true);
        }
    }
}

#[test]
fn paired_vertex_positions_are_invariant_in_every_assembly() {
    let stock = (ENVIRONMENT_WGSL, TERRAIN_WGSL);
    assert_family((stock.0.to_owned(), stock.1.to_owned()));
    for velocity in [false, true] {
        let (object, terrain) = if velocity {
            (
                crate::velocity::object_source(stock.0),
                crate::velocity::terrain_source(stock.1),
            )
        } else {
            (stock.0.to_owned(), stock.1.to_owned())
        };
        assert_family((object.clone(), terrain.clone()));
        assert_family((
            crate::ssao::ambient_source(&object),
            crate::ssao::ambient_source(&terrain),
        ));
        for ssao in [false, true] {
            for reflection in [false, true] {
                assert_family(crate::shadows::receiver_sources(ssao, reflection, velocity));
            }
        }
        for shadowed in [false, true] {
            for application_ao in [false, true] {
                assert_family(crate::ssr::forward::velocity_sources(
                    crate::shadows::capture_sources(shadowed),
                    velocity,
                    application_ao,
                ));
            }
        }
    }
    for output in [
        crate::scene_metadata::MetadataOutput::Normal,
        crate::scene_metadata::MetadataOutput::Merged,
    ] {
        assert_family(crate::scene_metadata::sources(output));
    }
}

#[test]
fn unpaired_positions_retain_original_compilation_contract() {
    use crate::renderer::depth::DepthMode;
    for source in [ENVIRONMENT_WGSL, TERRAIN_WGSL] {
        for mode in [DepthMode::Independent, DepthMode::Shared] {
            assert_positions(&mode.vertex_source(source), false);
        }
        for mode in [DepthMode::Prepass, DepthMode::Reuse] {
            assert_eq!(mode.vertex_source(source), source);
        }
    }
}

#[test]
fn directional_shadow_assembly_is_non_invariant() {
    let source = crate::shadows::depth_source();
    assert!(source.contains("fn vs_shadow("));
    assert_positions(&source, false);
}
