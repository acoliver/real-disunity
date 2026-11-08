//! Uniform local values, including Naga's lowered short-circuit temporaries.
use naga::{Expression, Function, Handle, LocalVariable, Module, Statement};
use std::collections::HashSet;

pub(super) fn uniform(module: &Module, f: &Function, h: Handle<Expression>) -> bool {
    let mut locals: HashSet<_> = f.local_variables.iter().map(|(h, _)| h).collect();
    loop {
        let mut bad = HashSet::new();
        for (h, local) in f.local_variables.iter() {
            if local
                .init
                .is_some_and(|init| !value(module, f, init, &locals))
            {
                bad.insert(h);
            }
        }
        writes(module, f, &f.body, &locals, false, &mut bad);
        let old = locals.len();
        locals.retain(|h| !bad.contains(h));
        if old == locals.len() {
            return value(module, f, h, &locals);
        }
    }
}

fn value(
    module: &Module,
    f: &Function,
    h: Handle<Expression>,
    locals: &HashSet<Handle<LocalVariable>>,
) -> bool {
    let uniform = |h| value(module, f, h, locals);
    match &f.expressions[h] {
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_) => true,
        Expression::GlobalVariable(h) => {
            module.global_variables[*h].space == naga::AddressSpace::Uniform
        }
        Expression::LocalVariable(h) => locals.contains(h),
        Expression::Load { pointer } => uniform(*pointer),
        Expression::AccessIndex { base, .. } => uniform(*base),
        Expression::Access { base, index } => uniform(*base) && uniform(*index),
        Expression::As { expr, .. } | Expression::Unary { expr, .. } => uniform(*expr),
        Expression::Binary { left, right, .. } => uniform(*left) && uniform(*right),
        Expression::Math {
            arg,
            arg1,
            arg2,
            arg3,
            ..
        } => uniform(*arg) && [*arg1, *arg2, *arg3].into_iter().flatten().all(uniform),
        Expression::Select {
            condition,
            accept,
            reject,
        } => uniform(*condition) && uniform(*accept) && uniform(*reject),
        Expression::Compose { components, .. } => components.iter().copied().all(uniform),
        Expression::Swizzle { vector, .. } => uniform(*vector),
        Expression::CallResult(handle) => accessor(module, &module.functions[*handle]),
        _ => false,
    }
}

fn accessor(module: &Module, f: &Function) -> bool {
    let Some(Statement::Return {
        value: Some(result),
    }) = f.body.last()
    else {
        return false;
    };
    f.arguments.is_empty()
        && f.body
            .iter()
            .all(|statement| matches!(statement, Statement::Emit(_) | Statement::Return { .. }))
        && uniform(module, f, *result)
}

fn writes(
    module: &Module,
    f: &Function,
    b: &naga::Block,
    locals: &HashSet<Handle<LocalVariable>>,
    divergent: bool,
    bad: &mut HashSet<Handle<LocalVariable>>,
) {
    for s in b {
        match s {
            Statement::Store { pointer, value: v } => {
                if let Some(h) = local_target(f, *pointer)
                    && (divergent
                        || !value(module, f, *v, locals)
                        || !value(module, f, *pointer, locals))
                {
                    bad.insert(h);
                }
            }
            Statement::Call { arguments, .. } => {
                bad.extend(arguments.iter().filter_map(|h| local_target(f, *h)));
            }
            Statement::If {
                condition,
                accept,
                reject,
            } => {
                let divergent = divergent || !value(module, f, *condition, locals);
                writes(module, f, accept, locals, divergent, bad);
                writes(module, f, reject, locals, divergent, bad);
            }
            _ => {
                let divergent =
                    divergent || matches!(s, Statement::Loop { .. } | Statement::Switch { .. });
                for b in super::branches(s) {
                    writes(module, f, b, locals, divergent, bad);
                }
            }
        }
    }
}

fn local_target(f: &Function, h: Handle<Expression>) -> Option<Handle<LocalVariable>> {
    match f.expressions[h] {
        Expression::LocalVariable(h) => Some(h),
        Expression::Access { base, .. } | Expression::AccessIndex { base, .. } => {
            local_target(f, base)
        }
        _ => None,
    }
}
