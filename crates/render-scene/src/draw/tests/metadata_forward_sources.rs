//! Complete forward source assemblies, including their final screen-space and terrain rewrites.

use std::collections::BTreeSet;

fn forward_source_cases() -> Vec<(String, (String, String))> {
    let stock = (crate::shader::ENVIRONMENT_WGSL, crate::shader::TERRAIN_WGSL);
    let mut cases = Vec::new();
    for velocity in [false, true] {
        for ssao in [false, true] {
            let (object, terrain) = if velocity {
                (
                    crate::velocity::object_source(stock.0),
                    crate::velocity::terrain_source(stock.1),
                )
            } else {
                (stock.0.to_owned(), stock.1.to_owned())
            };
            let sources = if ssao {
                (
                    crate::ssao::ambient_source(&object),
                    crate::ssao::ambient_source(&terrain),
                )
            } else {
                (object, terrain)
            };
            cases.push((format!("ordinary velocity={velocity} ssao={ssao}"), sources));
        }
    }
    for ssao in [false, true] {
        for reflection in [false, true] {
            for velocity in [false, true] {
                cases.push((
                    format!("receiver ssao={ssao} reflection={reflection} velocity={velocity}"),
                    crate::shadows::receiver_sources(ssao, reflection, velocity),
                ));
            }
        }
    }
    for shadowed in [false, true] {
        let stock = crate::shadows::capture_sources(shadowed);
        for velocity in [false, true] {
            for application_ao in [false, true] {
                cases.push((
                    format!("ssr shadowed={shadowed} velocity={velocity} application_ao={application_ao}"),
                    crate::ssr::forward::velocity_sources(stock, velocity, application_ao),
                ));
            }
        }
    }
    cases
}

fn final_forward_sources() -> Vec<(String, [String; 6])> {
    use worldrunner_render_plan::SceneSsaoReceiver::{DepthAwareGather, Nearest};
    let mut cases = Vec::new();
    for (label, (object, terrain)) in forward_source_cases() {
        let receivers: &[_] = if object.contains("fn screen_ao(") {
            &[Nearest, DepthAwareGather]
        } else {
            &[Nearest]
        };
        for &receiver in receivers {
            let object = crate::ssao::receiver::source(&object, receiver);
            let terrain = crate::ssao::receiver::source(&terrain, receiver);
            let [plain, cached, layered, layered_cached] =
                crate::terrain::program::source::test_sources(&terrain);
            cases.push((
                format!("{label} receiver={receiver:?}"),
                [object, terrain, plain, cached, layered, layered_cached],
            ));
        }
    }
    cases
}

#[test]
fn final_forward_assemblies_validate_and_expose_their_entry_points() {
    let object = super::entry_points("stock object", crate::shader::ENVIRONMENT_WGSL);
    let terrain = super::entry_points("stock terrain", crate::shader::TERRAIN_WGSL);
    let expected: [&BTreeSet<String>; 6] =
        [&object, &terrain, &terrain, &terrain, &terrain, &terrain];
    let cases = final_forward_sources();
    assert!(!cases.is_empty());
    super::check_in_parallel(&cases, |(label, sources)| {
        for (index, (source, required)) in sources.iter().zip(expected).enumerate() {
            super::assert_exposes(&format!("{label} source={index}"), source, required);
        }
    });
}

#[test]
fn unlit_quad_assemblies_validate_and_expose_their_entry_points() {
    let required = ["Vertex:vs_unlit", "Fragment:fs_unlit"]
        .map(str::to_owned)
        .into();
    let cases = final_forward_sources();
    super::check_in_parallel(&cases, |(label, sources)| {
        let source = crate::renderer::unlit_source(&sources[0]);
        super::assert_exposes(&format!("{label} unpaired quad source"), &source, &required);
    });
}
