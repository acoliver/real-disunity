//! What the runtime's storage budget is spent on, as report lines: counts and bytes by the kind
//! of resource, taken when the budget first refuses an allocation.

use std::collections::BTreeMap;

use worldrunner_render_program::runtime::{Holding, ProgramRuntime};

/// How many of the greatest groups are listed after the totals.
const LISTED: usize = 12;

/// The lines for the runtime's holdings: the budget, the bytes held and their peak, the total of
/// each label, then the greatest groups of one label and shape.
pub(super) fn describe(runtime: &ProgramRuntime) -> Vec<String> {
    lines(
        &runtime.holdings(),
        (
            runtime.budget().0,
            runtime.live_bytes(),
            runtime.peak_bytes(),
        ),
    )
}

fn lines(holdings: &[Holding], (budget, live, peak): (u64, u64, u64)) -> Vec<String> {
    let mut lines = vec![format!(
        "budget {budget} bytes, held {live}, peak {peak}, listed {}",
        holdings.iter().map(|holding| holding.bytes).sum::<u64>()
    )];
    let mut totals: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for holding in holdings {
        let total = totals.entry(&holding.label).or_default();
        total.0 += holding.count;
        total.1 += holding.bytes;
    }
    let mut totals: Vec<_> = totals.into_iter().collect();
    totals.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(b.0)));
    for (label, (count, bytes)) in totals {
        lines.push(format!("{label}: {count} allocations, {bytes} bytes"));
    }
    for holding in holdings.iter().take(LISTED) {
        let shape = if holding.detail.is_empty() {
            String::new()
        } else {
            format!(" {}", holding.detail)
        };
        lines.push(format!(
            "  {}{shape}: {} allocations, {} bytes, largest {}",
            holding.label, holding.count, holding.bytes, holding.largest
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(label: &str, detail: &str, count: usize, bytes: u64, largest: u64) -> Holding {
        Holding {
            label: label.to_owned(),
            detail: detail.to_owned(),
            count,
            bytes,
            largest,
        }
    }

    #[test]
    fn the_lines_total_each_label_and_list_the_greatest_groups() {
        let holdings = [
            holding("render texture", "8x8x1 R16Float 1 mips", 3, 384, 128),
            holding("render texture", "4x4x1 R8Unorm 1 mips", 2, 32, 16),
            holding("mesh", "", 5, 1000, 400),
        ];
        assert_eq!(
            lines(&holdings, (4096, 1416, 2000)),
            [
                "budget 4096 bytes, held 1416, peak 2000, listed 1416",
                "mesh: 5 allocations, 1000 bytes",
                "render texture: 5 allocations, 416 bytes",
                "  render texture 8x8x1 R16Float 1 mips: 3 allocations, 384 bytes, largest 128",
                "  render texture 4x4x1 R8Unorm 1 mips: 2 allocations, 32 bytes, largest 16",
                "  mesh: 5 allocations, 1000 bytes, largest 400",
            ]
        );
    }
}
