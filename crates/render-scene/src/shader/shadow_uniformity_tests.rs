// The guard deliberately forbids derivatives in loops, even uniform fixed-count loops.
// Reconverged branches without varying exits remain valid; varying data alone is permitted.
//! Naga 30 disables fragment derivative uniformity checks, so guard the shadow call graph.
use naga::{Expression, Function, Handle, Module, Statement};
use std::collections::HashSet;

#[path = "shadow_uniformity_values.rs"]
mod values;
use values::uniform;

fn branches(statement: &Statement) -> Vec<&naga::Block> {
    match statement {
        Statement::Block(b) => vec![b],
        Statement::If { accept, reject, .. } => vec![accept, reject],
        Statement::Loop {
            body, continuing, ..
        } => vec![body, continuing],
        Statement::Switch { cases, .. } => cases.iter().map(|c| &c.body).collect(),
        _ => vec![],
    }
}

fn exits(module: &Module, f: &Function, b: &naga::Block, divergent: bool) -> bool {
    b.iter().any(|s| match s {
        Statement::Kill => true,
        Statement::Return { .. } | Statement::Break | Statement::Continue => divergent,
        Statement::Call { function, .. } => kills(module, &module.functions[*function].body),
        Statement::If {
            condition,
            accept,
            reject,
        } => {
            let divergent = divergent || !uniform(module, f, *condition);
            exits(module, f, accept, divergent) || exits(module, f, reject, divergent)
        }
        _ => branches(s)
            .into_iter()
            .any(|b| exits(module, f, b, divergent)),
    })
}

fn kills(module: &Module, b: &naga::Block) -> bool {
    b.iter().any(|s| match s {
        Statement::Kill => true,
        Statement::Call { function, .. } => kills(module, &module.functions[*function].body),
        _ => branches(s).into_iter().any(|b| kills(module, b)),
    })
}

fn reachable(module: &Module, block: &naga::Block, seen: &mut HashSet<Handle<Function>>) {
    for statement in block {
        if let Statement::Call { function, .. } = statement
            && seen.insert(*function)
        {
            reachable(module, &module.functions[*function].body, seen);
        }
        for nested in branches(statement) {
            reachable(module, nested, seen);
        }
    }
}

fn calls(s: &Statement, roots: &HashSet<Handle<Function>>) -> bool {
    matches!(s, Statement::Call { function, .. } if roots.contains(function))
        || branches(s)
            .into_iter()
            .any(|b| b.iter().any(|s| calls(s, roots)))
}

fn shadow_paths(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    let mut subtree = HashSet::new();
    for (handle, f) in module.functions.iter().filter(|(_, f)| {
        matches!(
            f.name.as_deref(),
            Some(
                "raw_sunlight_visibility"
                    | "single_shadow_visibility"
                    | "scene_prepare_card_shadow"
                    | "water_filtered_raw_sunlight_visibility"
            )
        )
    }) {
        subtree.insert(handle);
        reachable(&module, &f.body, &mut subtree);
    }
    let mut roots: HashSet<_> = subtree
        .iter()
        .copied()
        .filter(|h| {
            module.functions[*h]
                .expressions
                .iter()
                .any(|(_, e)| derivative(e))
        })
        .collect();
    loop {
        let old = roots.len();
        for (h, f) in module.functions.iter() {
            if f.body.iter().any(|s| calls(s, &roots)) {
                roots.insert(h);
            }
        }
        if old == roots.len() {
            break;
        }
    }
    for (f, shadow) in module
        .functions
        .iter()
        .map(|(h, f)| (f, subtree.contains(&h)))
        .chain(module.entry_points.iter().map(|e| (&e.function, false)))
    {
        if !check_block(&module, f, &f.body, &roots, false, shadow) {
            return Err(f.name.clone().unwrap_or_default());
        }
    }
    Ok(())
}

fn derivative(e: &Expression) -> bool {
    matches!(
        e,
        Expression::Derivative { .. }
            | Expression::ImageSample {
                level: naga::SampleLevel::Auto | naga::SampleLevel::Bias(_),
                ..
            }
    )
}

fn check_block(
    module: &Module,
    f: &Function,
    b: &naga::Block,
    roots: &HashSet<Handle<Function>>,
    mut divergent: bool,
    shadow: bool,
) -> bool {
    for s in b {
        let sensitive = calls(s, roots)
            || matches!(s, Statement::Emit(r) if shadow && r.clone().any(|h| derivative(&f.expressions[h])));
        if divergent && sensitive {
            return false;
        }
        let nested_divergent = match s {
            Statement::If { condition, .. } => divergent || !uniform(module, f, *condition),
            Statement::Loop { .. } | Statement::Switch { .. } => true,
            _ => divergent,
        };
        if branches(s)
            .into_iter()
            .any(|b| !check_block(module, f, b, roots, nested_divergent, shadow))
        {
            return false;
        }
        divergent |= match s {
            Statement::Call { function, .. } => kills(module, &module.functions[*function].body),
            Statement::Kill => true,
            _ => branches(s)
                .into_iter()
                .any(|b| exits(module, f, b, nested_divergent)),
        };
    }
    true
}

#[test]
fn shadow_derivatives_precede_cascade_selection_and_divergent_exits() {
    for (terrain, base) in [
        (false, crate::shadows::capture_sources(true).0),
        (true, crate::shadows::capture_sources(true).1),
    ] {
        for source in [
            base.to_owned(),
            crate::ssao::ambient_source(base),
            crate::ssr::forward::replay_source(base, terrain),
        ] {
            assert_eq!(shadow_paths(&source), Ok(()));
        }
    }
    let water = crate::water::water_source(true);
    let filtered = format!(
        "{water}
{}",
        crate::water::shadow_filter::receiver_source()
    );
    for source in [water, filtered] {
        assert_eq!(shadow_paths(&source), Ok(()));
    }
}

#[test]
fn shadow_guard_tracks_nested_helpers_and_uniform_short_circuit_locals() {
    let prefix = "@group(0) @binding(0) var<uniform> flag:u32;";
    let safe = format!(
        "{prefix} fn raw_sunlight_visibility(p:vec3<f32>)->f32 {{ var enabled=flag!=0u; if enabled && flag!=1u {{ return dpdx(p.x); }} return 1.0; }}"
    );
    assert_eq!(shadow_paths(&safe), Ok(()));
    let accessor = safe.replace("var enabled=flag!=0u;", "var enabled=uniform_flag()!=0u;")
        + "
fn uniform_flag()->u32 { return flag; }";
    assert_eq!(shadow_paths(&accessor), Ok(()));
    let varying = accessor.replace(
        "fn uniform_flag()->u32 { return flag; }",
        "var<private> varying_flag:u32; fn uniform_flag()->u32 { return varying_flag; }",
    );
    assert!(shadow_paths(&varying).is_err());
    for assignment in ["enabled=p.x>0.0;", "if p.x>0.0 { enabled=false; }"] {
        assert!(
            shadow_paths(&safe.replace("if enabled &&", &format!("{assignment} if enabled &&")))
                .is_err()
        );
    }
    let indirect = "fn gradient(p:f32)->f32 { if p>0.0 { return dpdx(p); } return 0.0; } fn raw_sunlight_visibility(p:vec3<f32>)->f32 { return gradient(p.x); }";
    assert!(shadow_paths(indirect).is_err());
    let fragment = format!(
        "{indirect} @fragment fn fs(@location(0) p:vec3<f32>)->@location(0) f32 {{ return raw_sunlight_visibility(p); }}"
    );
    let module = naga::front::wgsl::parse_str(&fragment).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("Naga 30 does not enforce fragment derivative uniformity");
}

#[test]
fn shadow_guard_mutation_rejects_early_receiver_exit_in_the_assembled_shader() {
    let (object, terrain) = crate::shadows::capture_sources(true);
    for source in [object, terrain] {
        let mutation = source.replacen(
            "let receiver_dx = ",
            "if position.x > 0.0 { return 1.0; }
    let receiver_dx = ",
            1,
        );
        assert_ne!(source, mutation);
        let module = naga::front::wgsl::parse_str(&mutation).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert!(shadow_paths(&mutation).is_err());
    }
}

#[test]
fn shadow_guard_rejects_divergent_derivatives_and_implicit_samples() {
    for operation in ["dpdx(p.x)", "textureSample(tex, smp, p.xy).x"] {
        let prefix =
            "@group(0) @binding(0) var tex:texture_2d<f32>; @group(0) @binding(1) var smp:sampler;";
        for body in [
            format!("if p.x > 0.0 {{ return {operation}; }} return 1.0;"),
            format!("if p.x > 0.0 {{ return 1.0; }} return {operation};"),
        ] {
            let source =
                format!("{prefix} fn raw_sunlight_visibility(p:vec3<f32>)->f32 {{ {body} }}");
            assert!(shadow_paths(&source).is_err());
        }
    }
    let source = "fn raw_sunlight_visibility(p:vec3<f32>)->f32 { return dpdx(p.x); } @fragment fn fs(@location(0) p:vec3<f32>)->@location(0) f32 { if p.y > 0.0 { discard; } return raw_sunlight_visibility(p); }";
    assert!(shadow_paths(source).is_err());
}

#[test]
fn shadow_guard_rejects_indirect_local_writes_and_loop_switch_derivatives() {
    for body in [
        "var gate=vec2<u32>(0u); gate.x=u32(p.x>0.0); if gate.x!=0u { return dpdx(p.x); }",
        "var gate=array<u32,2>(0u,0u); gate[u32(p.x>0.0)]=1u; if gate[0]!=0u { return dpdx(p.x); }",
        "var gate=0u; assign_gate(&gate,p.x); if gate!=0u { return dpdx(p.x); }",
        "for(var i=0u;i<2u;i++){ if p.x>0.0 {break;} let d=dpdx(p.x); }",
        "switch u32(abs(p.x)) {case 0u:{return dpdx(p.x);} default:{}}",
    ] {
        let source = format!(
            "fn assign_gate(g:ptr<function,u32>,v:f32){{*g=u32(v>0.0);}} fn raw_sunlight_visibility(p:vec3<f32>)->f32 {{ {body} return 1.0; }} @fragment fn fs(@location(0) p:vec3<f32>)->@location(0) f32 {{ return raw_sunlight_visibility(p); }}"
        );
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert!(shadow_paths(&source).is_err(), "mutation accepted: {body}");
    }
}
