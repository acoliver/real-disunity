//! Scoped uniformity guard for atlas/coverage derivatives and their callers.
//!
//! Naga 30 disables fragment uniformity enforcement. This guard requires the
//! derivative-sensitive call chain to precede divergent control flow in each caller. It
//! permits branches proven to depend only on uniforms, and does not analyze unrelated
//! lighting/decal derivatives or prove value uniformity throughout the module.
#[path = "uniformity_values.rs"]
mod values;

use std::collections::HashSet;

use super::geometry_test_sources::geometry_sources;

use naga::{Expression, Function, Handle, Module, SampleLevel, Statement};

fn derivative_expression(expression: &Expression) -> bool {
    matches!(expression, Expression::Derivative { .. })
        || matches!(
            expression,
            Expression::ImageSample {
                level: SampleLevel::Auto | SampleLevel::Bias(_),
                ..
            }
        )
}

fn scoped(function: &Function) -> bool {
    let name = function.name.as_deref().unwrap_or("");
    name.starts_with("scene_atlas_")
        || name.starts_with("scene_relief_")
        || name.starts_with("scene_mask_")
        || name == "cotangent_frame"
}

fn calls_sensitive(block: &naga::Block, sensitive: &HashSet<Handle<Function>>) -> bool {
    block.iter().any(|statement| match statement {
        Statement::Call { function, .. } => sensitive.contains(function),
        Statement::If { accept, reject, .. } => {
            calls_sensitive(accept, sensitive) || calls_sensitive(reject, sensitive)
        }
        Statement::Block(block) => calls_sensitive(block, sensitive),
        Statement::Loop {
            body, continuing, ..
        } => calls_sensitive(body, sensitive) || calls_sensitive(continuing, sensitive),
        Statement::Switch { cases, .. } => cases
            .iter()
            .any(|case| calls_sensitive(&case.body, sensitive)),
        _ => false,
    })
}

fn sensitive_functions(module: &Module) -> HashSet<Handle<Function>> {
    let mut sensitive: HashSet<_> = module
        .functions
        .iter()
        .filter(|(_, function)| {
            scoped(function)
                && function
                    .expressions
                    .iter()
                    .any(|(_, e)| derivative_expression(e))
        })
        .map(|(handle, _)| handle)
        .collect();
    loop {
        let count = sensitive.len();
        for (handle, function) in module.functions.iter() {
            if calls_sensitive(&function.body, &sensitive) {
                sensitive.insert(handle);
            }
        }
        if sensitive.len() == count {
            return sensitive;
        }
    }
}

fn block_kills(block: &naga::Block) -> bool {
    block.iter().any(|statement| match statement {
        Statement::Kill => true,
        Statement::If { accept, reject, .. } => block_kills(accept) || block_kills(reject),
        Statement::Block(block) => block_kills(block),
        Statement::Loop {
            body, continuing, ..
        } => block_kills(body) || block_kills(continuing),
        Statement::Switch { cases, .. } => cases.iter().any(|case| block_kills(&case.body)),
        _ => false,
    })
}

fn terminating_functions(module: &Module) -> HashSet<Handle<Function>> {
    let mut terminating: HashSet<_> = module
        .functions
        .iter()
        .filter(|(_, function)| block_kills(&function.body))
        .map(|(handle, _)| handle)
        .collect();
    loop {
        let count = terminating.len();
        for (handle, function) in module.functions.iter() {
            if calls_sensitive(&function.body, &terminating) {
                terminating.insert(handle);
            }
        }
        if terminating.len() == count {
            return terminating;
        }
    }
}

struct Guard<'a> {
    module: &'a Module,
    sensitive: HashSet<Handle<Function>>,
    terminating: HashSet<Handle<Function>>,
}

impl Guard<'_> {
    fn check_block(&self, function: &Function, block: &naga::Block, mut divergent: bool) -> bool {
        for statement in block {
            let needs_quad = match statement {
                Statement::Emit(range) if scoped(function) => range
                    .clone()
                    .any(|handle| derivative_expression(&function.expressions[handle])),
                Statement::Call { function, .. } => self.sensitive.contains(function),
                _ => false,
            };
            if needs_quad && divergent {
                return false;
            }
            if !self.nested_blocks_valid(function, statement, divergent) {
                return false;
            }
            divergent |= self.loses_uniformity(function, statement);
        }
        true
    }

    fn loses_uniformity(&self, function: &Function, statement: &Statement) -> bool {
        match statement {
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                !values::uniform_value(self.module, function, *condition)
                    || self.varying_exit(function, accept, false)
                    || self.varying_exit(function, reject, false)
            }
            Statement::Block(_)
            | Statement::Switch { .. }
            | Statement::Loop { .. }
            | Statement::Return { .. }
            | Statement::Kill => true,
            Statement::Call { function, .. } => self.terminating.contains(function),
            _ => false,
        }
    }

    fn varying_exit(&self, function: &Function, block: &naga::Block, divergent: bool) -> bool {
        block.iter().any(|statement| match statement {
            Statement::Return { .. } => divergent,
            Statement::Kill => true,
            Statement::Call { function, .. } => self.terminating.contains(function),
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                let divergent =
                    divergent || !values::uniform_value(self.module, function, *condition);
                self.varying_exit(function, accept, divergent)
                    || self.varying_exit(function, reject, divergent)
            }
            Statement::Block(block) => self.varying_exit(function, block, divergent),
            Statement::Loop {
                body, continuing, ..
            } => {
                self.varying_exit(function, body, true)
                    || self.varying_exit(function, continuing, true)
            }
            Statement::Switch { cases, .. } => cases
                .iter()
                .any(|case| self.varying_exit(function, &case.body, true)),
            _ => false,
        })
    }

    fn nested_blocks_valid(
        &self,
        function: &Function,
        statement: &Statement,
        divergent: bool,
    ) -> bool {
        match statement {
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                let divergent =
                    divergent || !values::uniform_value(self.module, function, *condition);
                self.check_block(function, accept, divergent)
                    && self.check_block(function, reject, divergent)
            }
            Statement::Block(block) => self.check_block(function, block, divergent),
            Statement::Loop {
                body, continuing, ..
            } => {
                self.check_block(function, body, true)
                    && self.check_block(function, continuing, true)
            }
            Statement::Switch { cases, .. } => cases
                .iter()
                .all(|case| self.check_block(function, &case.body, true)),
            _ => true,
        }
    }
}

fn uniform_atlas_paths(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let guard = Guard {
        module: &module,
        sensitive: sensitive_functions(&module),
        terminating: terminating_functions(&module),
    };
    let functions = module
        .functions
        .iter()
        .map(|(_, function)| function)
        .chain(module.entry_points.iter().map(|entry| &entry.function));
    for function in functions {
        if !guard.check_block(function, &function.body, false) {
            return Err(function.name.clone().unwrap_or_default());
        }
    }
    Ok(())
}

#[test]
fn atlas_and_coverage_derivatives_precede_control_flow_in_all_passes() {
    for source in geometry_sources() {
        assert_eq!(uniform_atlas_paths(&source), Ok(()));
    }
    // The merged prepass writes the normal and the reflection material from one evaluation, so
    // its entries need the same guarantee as the single-output metadata pass.
    let merged = crate::scene_metadata::MetadataOutput::Merged.object_fragment();
    let base = geometry_sources()[1]
        .split_once(&crate::scene_metadata::single_fragment(
            "vec4<f32>(normal, 1.0)",
        ))
        .expect("metadata fixture")
        .0
        .to_owned();
    let camera = include_str!("../ssao/camera.wgsl");
    assert_eq!(
        uniform_atlas_paths(&format!("{base}\n{camera}\n{merged}")),
        Ok(())
    );
}

#[test]
fn guard_rejects_derivatives_after_fragment_dependent_returns_and_calls() {
    let source = "fn scene_atlas_bad(value: f32) -> f32 { if value == 0.0 { return 0.0; } return dpdx(value); }";
    assert!(uniform_atlas_paths(source).is_err());
    let source = "fn scene_atlas_gradient(value: f32) -> f32 { return dpdx(value); } fn normal(value: f32) -> f32 { if value == 0.0 { return 0.0; } return scene_atlas_gradient(value); }";
    assert!(uniform_atlas_paths(source).is_err());
    let source = "fn scene_atlas_gradient(value: f32) -> f32 { return dpdx(value); } fn normal(value: f32) -> f32 { if value != 0.0 { return scene_atlas_gradient(value); } return 0.0; }";
    assert!(uniform_atlas_paths(source).is_err());
}

#[test]
fn guard_rejects_implicit_samples_and_derivatives_after_callee_discard() {
    let source = "@group(0) @binding(0) var tex: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn scene_atlas_bad(uv: vec2<f32>) -> vec4<f32> { if uv.x == 0.0 { return vec4<f32>(0.0); } return textureSampleBias(tex, smp, uv, 0.0); }";
    assert!(uniform_atlas_paths(source).is_err());
    let source = "fn kill(value: f32) { if value == 0.0 { discard; } } fn scene_atlas_gradient(value: f32) -> f32 { return dpdx(value); } fn normal(value: f32) -> f32 { kill(value); return scene_atlas_gradient(value); }";
    assert!(uniform_atlas_paths(source).is_err());
}

#[test]
fn guard_permits_uniform_feature_gates_and_explicit_samples_in_divergent_flow() {
    let source = "@group(0) @binding(0) var<uniform> flag: u32; fn scene_atlas_uniform(value: f32) -> f32 { if flag == 0u { return 0.0; } return dpdx(value); }";
    assert_eq!(uniform_atlas_paths(source), Ok(()));
    let source = "@group(0) @binding(0) var tex: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn scene_atlas_explicit(uv: vec2<f32>) -> vec4<f32> { let dx = dpdx(uv); let dy = dpdy(uv); if uv.x == 0.0 { return textureSampleGrad(tex, smp, uv, dx, dy); } return textureSampleLevel(tex, smp, uv, 0.0); }";
    assert_eq!(uniform_atlas_paths(source), Ok(()));
    let source = "@group(0) @binding(0) var<uniform> flag: u32; fn scene_atlas_bad(value: f32) -> f32 { if value == 0.0 { return 0.0; } if flag != 0u { return dpdx(value); } return 0.0; }";
    assert!(uniform_atlas_paths(source).is_err());
}

#[test]
fn uniform_gates_do_not_hide_nested_divergent_returns_or_discards() {
    let source = "@group(0) @binding(0) var<uniform> flag: u32; fn scene_atlas_bad(value: f32) -> f32 { if flag != 0u { if value == 0.0 { return 0.0; } } return dpdx(value); }";
    assert!(uniform_atlas_paths(source).is_err());
    let source = "@group(0) @binding(0) var<uniform> flag: u32; fn kill(value: f32) { if value == 0.0 { discard; } } fn scene_atlas_bad(value: f32) -> f32 { if flag != 0u { kill(value); } return dpdx(value); }";
    assert!(uniform_atlas_paths(source).is_err());
}
