//! Terrain filtering must not depend on neighbouring fragment execution.
//! Decal projection and lighting own independent derivatives outside material filtering.
use naga::{Expression, Function, Handle, Module, Statement};
use std::collections::HashSet;

fn called_functions(block: &naga::Block, calls: &mut Vec<Handle<Function>>) {
    for statement in block {
        match statement {
            Statement::Call { function, .. } => calls.push(*function),
            Statement::Block(block) => called_functions(block, calls),
            Statement::If { accept, reject, .. } => {
                called_functions(accept, calls);
                called_functions(reject, calls);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                called_functions(body, calls);
                called_functions(continuing, calls);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    called_functions(&case.body, calls);
                }
            }
            _ => {}
        }
    }
}

fn check_function(
    module: &Module,
    function: &Function,
    seen: &mut HashSet<Handle<Function>>,
) -> bool {
    if matches!(
        function.name.as_deref(),
        Some("scene_project_decals" | "scene_surface_lighting")
    ) {
        return true;
    }
    if function
        .expressions
        .iter()
        .any(|(_, e)| matches!(e, Expression::Derivative { .. }))
    {
        eprintln!("hardware derivative reached in {:?}", function.name);
        return false;
    }
    let mut calls = Vec::new();
    called_functions(&function.body, &mut calls);
    calls.into_iter().all(|handle| {
        !seen.insert(handle) || check_function(module, &module.functions[handle], seen)
    })
}

fn deterministic_terrain(source: &str) -> bool {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let fragments: Vec<_> = module
        .entry_points
        .iter()
        .filter(|entry| entry.stage == naga::ShaderStage::Fragment)
        .collect();
    assert!(!fragments.is_empty());
    fragments
        .iter()
        .all(|entry| check_function(&module, &entry.function, &mut HashSet::new()))
}

#[test]
fn terrain_footprints_are_derivative_free_in_every_fragment_pass() {
    use crate::scene_metadata::MetadataOutput;
    let base = super::TERRAIN_WGSL;
    let camera = include_str!("../ssao/camera.wgsl");
    let mut sources = vec![
        base.to_owned(),
        crate::ssr::forward::source(base, true),
        crate::ssr::forward::replay_source(base, true),
        crate::velocity::terrain_source(base),
    ];
    sources.extend(
        [
            MetadataOutput::Normal,
            MetadataOutput::Material,
            MetadataOutput::Merged,
        ]
        .map(|output| {
            format!(
                "{}
{camera}",
                output.terrain_source(base)
            )
        }),
    );
    sources.extend(
        [false, true].map(|shadowed| crate::shadows::capture_sources(shadowed).1.to_owned()),
    );
    for source in sources {
        for variant in crate::renderer::terrain_test_sources(&source) {
            assert!(deterministic_terrain(&variant));
        }
    }
}

#[test]
fn guard_rejects_direct_and_nested_world_position_derivatives() {
    for builtin in ["dpdx", "dpdy", "dpdxFine", "dpdyCoarse", "fwidth"] {
        let direct = format!(
            "@fragment fn fs_terrain(@location(0) p: vec3<f32>) -> @location(0) vec4<f32> {{ return vec4<f32>({builtin}(p), 1.0); }}"
        );
        assert!(!deterministic_terrain(&direct));
        let nested = format!(
            "fn footprint(p: vec3<f32>) -> vec3<f32> {{ return {builtin}(p); }} @fragment fn fs_terrain(@location(0) p: vec3<f32>) -> @location(0) vec4<f32> {{ if p.x > 0.0 {{ return vec4<f32>(footprint(p), 1.0); }} return vec4<f32>(0.0); }}"
        );
        assert!(!deterministic_terrain(&nested));
    }
    assert!(!deterministic_terrain(&super::TERRAIN_WGSL.replace(
        "let world_xy_dx = terrain_quad_footprint(input.clip_position, vec2<f32>(1.0, 0.0), footprint_normal);",
        "let world_xy_dx = dpdx(world_xy);")));
}
