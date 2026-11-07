//! Transitive forward guard: rejection must not precede any quad-sensitive work.
use super::*;

fn safe_block(
    guard: &Guard<'_>,
    function: &Function,
    block: &naga::Block,
    mut rejected: bool,
) -> bool {
    for statement in block {
        let sensitive = match statement {
            Statement::Emit(range) => range
                .clone()
                .any(|h| derivative_expression(&function.expressions[h])),
            Statement::Call { function, .. } => guard.sensitive.contains(function),
            _ => false,
        };
        if rejected && sensitive {
            return false;
        }
        let valid = match statement {
            Statement::If { accept, reject, .. } => {
                safe_block(guard, function, accept, rejected)
                    && safe_block(guard, function, reject, rejected)
            }
            Statement::Block(body) => safe_block(guard, function, body, rejected),
            Statement::Loop {
                body, continuing, ..
            } => {
                let rejected = rejected
                    || block_kills(body)
                    || calls_sensitive(body, &guard.terminating)
                    || block_kills(continuing)
                    || calls_sensitive(continuing, &guard.terminating);
                safe_block(guard, function, body, rejected)
                    && safe_block(guard, function, continuing, rejected)
            }
            Statement::Switch { cases, .. } => cases
                .iter()
                .all(|case| safe_block(guard, function, &case.body, rejected)),
            _ => true,
        };
        if !valid {
            return false;
        }
        rejected |= match statement {
            Statement::Kill => true,
            Statement::Call { function, .. } => guard.terminating.contains(function),
            Statement::If { accept, reject, .. } => {
                block_kills(accept)
                    || block_kills(reject)
                    || calls_sensitive(accept, &guard.terminating)
                    || calls_sensitive(reject, &guard.terminating)
            }
            Statement::Block(body) => {
                block_kills(body) || calls_sensitive(body, &guard.terminating)
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                block_kills(body)
                    || calls_sensitive(body, &guard.terminating)
                    || block_kills(continuing)
                    || calls_sensitive(continuing, &guard.terminating)
            }
            Statement::Switch { cases, .. } => cases.iter().any(|case| {
                block_kills(&case.body) || calls_sensitive(&case.body, &guard.terminating)
            }),
            _ => false,
        };
    }
    true
}

fn check_function(
    guard: &Guard<'_>,
    function: &Function,
    visited: &mut HashSet<Handle<Function>>,
) -> bool {
    if !safe_block(guard, function, &function.body, false) {
        return false;
    }
    let mut calls = Vec::new();
    collect_calls(&function.body, &mut calls);
    calls.into_iter().all(|handle| {
        !visited.insert(handle) || check_function(guard, &guard.module.functions[handle], visited)
    })
}

fn collect_calls(block: &naga::Block, calls: &mut Vec<Handle<Function>>) {
    for statement in block {
        match statement {
            Statement::Call { function, .. } => calls.push(*function),
            Statement::If { accept, reject, .. } => {
                collect_calls(accept, calls);
                collect_calls(reject, calls);
            }
            Statement::Block(body) => collect_calls(body, calls),
            Statement::Loop {
                body, continuing, ..
            } => {
                collect_calls(body, calls);
                collect_calls(continuing, calls);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    collect_calls(&case.body, calls);
                }
            }
            _ => (),
        }
    }
}

fn check(source: &str, root: &str) -> bool {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let guard = Guard {
        module: &module,
        all_derivatives: true,
        sensitive: sensitive_functions(&module, true),
        terminating: terminating_functions(&module),
    };
    let function = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some(root))
        .map(|(_, f)| f)
        .or_else(|| {
            module
                .entry_points
                .iter()
                .find(|e| e.name == root)
                .map(|e| &e.function)
        })
        .unwrap();
    check_function(&guard, function, &mut HashSet::new())
}

#[test]
fn forward_term_captures_have_no_derivatives_after_card_rejection() {
    use crate::compositor::terms::{CaptureTerm, term_sources};
    let mut failures = Vec::new();
    for shadowed in [false, true] {
        for term in [
            CaptureTerm::Albedo,
            CaptureTerm::SunVisibility,
            CaptureTerm::DirectDiffuse,
            CaptureTerm::AmbientDiffuse,
            CaptureTerm::EnvironmentSpecular,
            CaptureTerm::SunSpecular,
        ] {
            let object = term_sources(crate::shadows::capture_sources(shadowed), term).0;
            let ambient = crate::ssao::ambient_source(&object);
            let reflection = crate::ssr::forward::replay_source(&ambient, false);
            for source in [object, ambient, reflection] {
                for entry in [
                    "fs_opaque",
                    "fs_masked",
                    "fs_relief_opaque",
                    "fs_relief_masked",
                ] {
                    if !check(&source, entry) {
                        failures.push(format!("{term:?}, {entry}, shadowed={shadowed}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn forward_card_rejection_precedes_auxiliary_and_lighting_without_late_derivatives() {
    for shadowed in [false, true] {
        let source = crate::shadows::capture_sources(shadowed).0;
        assert!(
            source.contains("fn scene_card_shade("),
            "staged card shading is missing"
        );
        assert!(check(source, "scene_card_shade"));
        let stock = crate::shadows::capture_sources(shadowed);
        let (reflection, _) = crate::ssr::forward::velocity_sources(stock, true, true);
        for source in [
            source.to_owned(),
            crate::velocity::object_source(source),
            reflection,
        ] {
            for entry in [
                "fs_opaque",
                "fs_masked",
                "fs_relief_opaque",
                "fs_relief_masked",
            ] {
                assert!(check(&source, entry), "{entry}, shadowed={shadowed}");
            }
        }
    }
}

#[test]
fn forward_guard_checks_reachable_helper_bodies_and_implicit_samples() {
    let source = "fn mapped(v:f32)->f32 { if v==0.0 { discard; } return dpdx(v); } @fragment fn fs_masked(@location(0) v:f32)->@location(0) f32 { return mapped(v); }";
    assert!(!check(source, "fs_masked"));
    let source = "@group(0) @binding(0) var tex:texture_2d<f32>; @group(0) @binding(1) var smp:sampler; fn mapped(v:vec2<f32>)->vec4<f32> { if v.x==0.0 { discard; } return textureSample(tex,smp,v); } @fragment fn fs_masked(@location(0) v:vec2<f32>)->@location(0) vec4<f32> { return mapped(v); }";
    assert!(!check(source, "fs_masked"));
}

#[test]
fn forward_rejection_defers_auxiliary_reads_and_lighting() {
    let module = naga::front::wgsl::parse_str(crate::shader::ENVIRONMENT_WGSL).unwrap();
    let function = module
        .functions
        .iter()
        .find(|(_, f)| f.name.as_deref() == Some("scene_card_shade"))
        .unwrap()
        .1;
    let mut rejected = false;
    let mut deferred = 0;
    for statement in &function.body {
        if let Statement::If { accept, reject, .. } = statement {
            rejected |= block_kills(accept) || block_kills(reject);
        }
        if let Statement::Call { function, .. } = statement {
            let name = module.functions[*function].name.as_deref().unwrap();
            if matches!(
                name,
                "scene_occlusion_sample_prepared"
                    | "scene_emission_sample_prepared"
                    | "scene_card_finish_shading"
            ) {
                assert!(rejected, "{name} must follow rejection");
                deferred += 1;
            }
        }
    }
    assert_eq!(deferred, 3);
}
