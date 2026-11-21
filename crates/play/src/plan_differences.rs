//! The plan differences that say why a material has no shader, printed at startup.
//!
//! The plan leaves a material's shader null when the material names none or its shader did not
//! plan, and the managed side then builds the material with its fallback shader. The cause is
//! only in the plan's differences, so the player prints those that concern a shader.

use std::collections::BTreeSet;

/// How many distinct shader differences are printed before the rest are only counted.
const SHOWN: usize = 40;

/// Whether `difference` is about a shader: a shader that did not plan, or a material whose
/// shader reference is missing or unresolved. Compute shaders have their own differences.
fn concerns_shader(difference: &str) -> bool {
    if difference.starts_with("shader did not plan") {
        return true;
    }
    difference
        .strip_prefix("material ")
        .and_then(|rest| rest.split_once(": "))
        .is_some_and(|(_, cause)| {
            cause.starts_with("shader ") || cause.starts_with("references no shader")
        })
}

/// The lines to print for the shader differences among `differences`: each distinct one in
/// first-seen order, up to a cap, then a line counting the rest. Empty when there are none.
pub(crate) fn shader_difference_lines(differences: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let distinct: Vec<&String> = differences
        .iter()
        .filter(|difference| concerns_shader(difference) && seen.insert(difference.as_str()))
        .collect();
    let mut lines: Vec<String> = distinct
        .iter()
        .take(SHOWN)
        .map(|difference| format!("[player] shader: {difference}"))
        .collect();
    if distinct.len() > SHOWN {
        lines.push(format!(
            "[player] shader: and {} more differences",
            distinct.len() - SHOWN
        ));
    }
    lines
}

#[cfg(test)]
#[path = "plan_differences_tests.rs"]
mod tests;
