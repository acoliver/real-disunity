//! Deliberately narrow proof for conditions built only from uniform values.
use naga::{AddressSpace, Expression, Function, Handle, Module};

pub(super) fn uniform_value(
    module: &Module,
    function: &Function,
    value: Handle<Expression>,
) -> bool {
    let uniform = |value| uniform_value(module, function, value);
    match function.expressions[value] {
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_) => true,
        Expression::GlobalVariable(global) => {
            matches!(module.global_variables[global].space, AddressSpace::Uniform)
        }
        Expression::Load { pointer } => uniform(pointer),
        Expression::AccessIndex { base, .. } => uniform(base),
        Expression::Access { base, index } => uniform(base) && uniform(index),
        Expression::Unary { expr, .. } => uniform(expr),
        Expression::Binary { left, right, .. } => uniform(left) && uniform(right),
        _ => false,
    }
}
