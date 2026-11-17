//! CPU checks of final object producer assemblies and their contour siblings.
use super::{ENVIRONMENT_WGSL, position_dependencies::graph};

fn check(producer: &str) {
    let contour = format!("{producer}\n{}", include_str!("contour.wgsl"));
    assert_eq!(graph(producer, "vs_main"), graph(&contour, "vs_main"));
    let module = naga::front::wgsl::parse_str(&contour)
        .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&contour)));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let vertex = module
        .entry_points
        .iter()
        .find(|ep| ep.name == "vs_main")
        .unwrap();
    let naga::TypeInner::Struct { members, .. } =
        &module.types[vertex.function.result.as_ref().unwrap().ty].inner
    else {
        panic!("object positions use struct output");
    };
    assert!(members.iter().any(|member| {
        member.binding
            == Some(naga::Binding::BuiltIn(naga::BuiltIn::Position {
                invariant: true,
            }))
    }));
    assert_eq!(
        module
            .global_variables
            .iter()
            .filter(|(_, variable)| {
                variable.binding
                    == Some(naga::ResourceBinding {
                        group: 2,
                        binding: 30,
                    })
            })
            .count(),
        1,
        "the contour uniform must not collide with producer bindings"
    );
}

#[test]
fn contour_final_sources_keep_invariant_positions_and_unique_id_binding() {
    let mut producers = vec![ENVIRONMENT_WGSL.to_owned()];
    for velocity in [false, true] {
        let stock = if velocity {
            crate::velocity::object_source(ENVIRONMENT_WGSL)
        } else {
            ENVIRONMENT_WGSL.to_owned()
        };
        producers.push(stock.clone());
        producers.push(crate::ssao::ambient_source(&stock));
        for ssao in [false, true] {
            for reflection in [false, true] {
                producers.push(crate::shadows::receiver_sources(ssao, reflection, velocity).0);
            }
        }
    }
    for producer in producers {
        check(&producer);
        if !producer.contains("struct PreviousInstance") {
            check(&crate::object::skin_source::skinned_source(&producer));
        }
    }
}
