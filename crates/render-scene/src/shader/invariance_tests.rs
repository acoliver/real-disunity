//! Qualifier coverage across paired source assemblies.
use super::{ENVIRONMENT_WGSL, TERRAIN_WGSL};

fn assert_positions(source: &str, entries: &[&str], invariant: bool) {
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
        let object = crate::ssao::receiver::source(&object, receiver);
        assert_positions(&object, &["vs_main"], true);
        assert_eq!(
            super::position_dependencies::graph(&object, "vs_main"),
            super::position_dependencies::graph(ENVIRONMENT_WGSL, "vs_main")
        );
        let terrain = crate::ssao::receiver::source(&terrain, receiver);
        for source in crate::terrain::program::source::test_sources(&terrain) {
            assert_positions(&source, &["vs_terrain"], true);
            assert_eq!(
                super::position_dependencies::graph(&source, "vs_terrain"),
                super::position_dependencies::graph(TERRAIN_WGSL, "vs_terrain")
            );
        }
    }
}

#[test]
fn paired_assemblies_have_invariant_matching_position_dependency_graphs() {
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
fn stock_paired_positions_are_invariant() {
    assert_positions(ENVIRONMENT_WGSL, &["vs_main"], true);
    assert_positions(TERRAIN_WGSL, &["vs_terrain"], true);
    assert_positions(
        &crate::renderer::unlit_source(ENVIRONMENT_WGSL),
        &["vs_unlit"],
        false,
    );
}

#[test]
fn directional_shadow_assembly_is_non_invariant() {
    let source = crate::shadows::depth_source();
    assert!(source.contains("fn vs_shadow("));
    assert_positions(&source, &["vs_shadow", "vs_terrain_depth"], false);
}
