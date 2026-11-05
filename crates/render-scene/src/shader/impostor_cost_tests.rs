//! Call-tree cost bounds for surface specialization and ordinary material uniforms.
use naga::{BinaryOperator, Expression, Function, Handle, Module, Statement};

use super::geometry_test_sources::geometry_sources;

fn material_member(
    module: &Module,
    function: &Function,
    value: Handle<Expression>,
) -> Option<String> {
    match &function.expressions[value] {
        Expression::GlobalVariable(global) => module.global_variables[*global].name.clone(),
        Expression::AccessIndex { base, index } => Some(format!(
            "{}.{index}",
            material_member(module, function, *base)?
        )),
        Expression::Load { pointer } => material_member(module, function, *pointer),
        _ => None,
    }
}

fn ordinary_zero(module: &Module, function: &Function, value: Handle<Expression>) -> Option<bool> {
    if let Some(member) = material_member(module, function, value) {
        // Material field indices match the parsed Material type, not source spelling.
        let global = module
            .global_variables
            .iter()
            .find(|(_, g)| g.name.as_deref() == Some("material"))?
            .1;
        let naga::TypeInner::Struct { members, .. } = &module.types[global.ty].inner else {
            return None;
        };
        for (index, field) in members.iter().enumerate() {
            let zero = match field.name.as_deref()? {
                "impostor_grid" | "impostor_relief" => {
                    member.starts_with(&format!("material.{index}."))
                }
                "wind_clock" => member == format!("material.{index}.2"),
                _ => false,
            };
            if zero {
                return Some(true);
            }
        }
    }
    match function.expressions[value] {
        Expression::Literal(naga::Literal::F32(v)) => Some(v.abs() <= 0.0),
        Expression::Literal(naga::Literal::U32(v)) => Some(v == 0),
        Expression::Binary {
            op: BinaryOperator::And,
            left,
            right,
        } => {
            if ordinary_zero(module, function, left) == Some(true)
                || ordinary_zero(module, function, right) == Some(true)
            {
                Some(true)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn condition(module: &Module, function: &Function, value: Handle<Expression>) -> Option<bool> {
    match function.expressions[value] {
        Expression::Literal(naga::Literal::Bool(v)) => Some(v),
        Expression::Constant(c) => match module.global_expressions[module.constants[c].init] {
            Expression::Literal(naga::Literal::Bool(v)) => Some(v),
            _ => None,
        },
        Expression::Unary {
            op: naga::UnaryOperator::LogicalNot,
            expr,
        } => Some(!condition(module, function, expr)?),
        Expression::Binary { op, left, right } => {
            let a = condition(module, function, left);
            let b = condition(module, function, right);
            match op {
                BinaryOperator::LogicalAnd if a == Some(false) || b == Some(false) => Some(false),
                BinaryOperator::LogicalOr if a == Some(true) || b == Some(true) => Some(true),
                BinaryOperator::Equal | BinaryOperator::NotEqual => {
                    let both_zero = ordinary_zero(module, function, left)?
                        && ordinary_zero(module, function, right)?;
                    both_zero.then_some(op == BinaryOperator::Equal)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn forbidden_call(module: &Module, function: &Function, arguments: &[Handle<Expression>]) -> bool {
    arguments.iter().any(|argument| {
        matches!(function.expressions[*argument], Expression::GlobalVariable(g)
            if module.global_variables[g].name.as_deref() == Some("impostor_depth"))
    })
}

fn visit_call(
    module: &Module,
    function: &Function,
    target: Handle<Function>,
    arguments: &[Handle<Expression>],
    work: &mut Vec<String>,
) {
    let callee = &module.functions[target];
    let name = callee.name.as_deref().unwrap_or_default();
    if matches!(
        name,
        "scene_atlas_coordinates"
            | "scene_atlas_selection"
            | "scene_relief_normal"
            | "scene_atlas_first"
    ) || forbidden_call(module, function, arguments)
    {
        work.push(name.to_owned());
    }
    visit(module, callee, &callee.body, work);
}

fn visit_if(
    module: &Module,
    function: &Function,
    branches: (Handle<Expression>, &naga::Block, &naga::Block),
    work: &mut Vec<String>,
) -> bool {
    let (test, accept, reject) = branches;
    match condition(module, function, test) {
        Some(true) => visit(module, function, accept, work),
        Some(false) => visit(module, function, reject, work),
        None => {
            let a = visit(module, function, accept, work);
            let b = visit(module, function, reject, work);
            a && b
        }
    }
}

fn visit(
    module: &Module,
    function: &Function,
    block: &naga::Block,
    work: &mut Vec<String>,
) -> bool {
    for statement in block {
        let exits = match statement {
            Statement::Call {
                function: target,
                arguments,
                ..
            } => {
                visit_call(module, function, *target, arguments, work);
                false
            }
            Statement::If {
                condition,
                accept,
                reject,
            } => visit_if(module, function, (*condition, accept, reject), work),
            Statement::Block(block) => visit(module, function, block, work),
            Statement::Loop {
                body, continuing, ..
            } => {
                visit(module, function, body, work);
                visit(module, function, continuing, work);
                false
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    visit(module, function, &case.body, work);
                }
                false
            }
            Statement::Return { .. } | Statement::Kill => true,
            _ => false,
        };
        if exits {
            return true;
        }
    }
    false
}

fn assert_no_atlas_work(module: &Module) {
    for entry in module.entry_points.iter().filter(|e| {
        matches!(
            e.name.as_str(),
            "fs_opaque" | "fs_masked" | "fs_depth_masked" | "fs_depth_opaque"
        )
    }) {
        let mut work = Vec::new();
        visit(module, &entry.function, &entry.function.body, &mut work);
        assert!(
            work.is_empty(),
            "{} executes impostor work: {work:?}",
            entry.name
        );
    }
}

pub(crate) fn assert_ordinary_material_cost() {
    for source in geometry_sources() {
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        assert_no_atlas_work(&module);
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let key = crate::pipeline::PipelineKey::ALL[0];
        assert!(!key.sampling.is_impostor());
        let constants = key
            .fragment_constants(1)
            .into_iter()
            .map(|(name, value)| (String::from(name), value))
            .collect();
        let (specialized, _) =
            naga::back::pipeline_constants::process_overrides(&module, &info, None, &constants)
                .unwrap();
        let mut specialized = specialized.into_owned();
        // The specialization must remove atlas work without assuming material lane values.
        for (_, global) in specialized.global_variables.iter_mut() {
            if global.name.as_deref() == Some("material") {
                global.name = Some(String::from("unconstrained_material"));
            }
        }
        assert_no_atlas_work(&specialized);
    }
}

#[test]
fn ordinary_opaque_and_masked_materials_do_no_impostor_sampling_work() {
    assert_ordinary_material_cost();
}
