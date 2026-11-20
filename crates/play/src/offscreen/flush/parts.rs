//! The parts a run of records is built in.

use std::ops::Range;

use worldrunner_render_program::unity::offscreen::frame::{OffscreenFrame, Record};

/// Splits `run`, a range of `records`, into consecutive parts that each plan at most `budget`
/// draws, by the bound `frame` gives for each record. A part is closed before the record that
/// would take it over the budget. A record that plans more than the budget alone is a part of
/// its own, so a part holds more than the budget only when one record does.
///
/// The parts cover `run` in order. An empty run is one empty part, which the flush still builds
/// when it is the first, since the first build makes the textures the dispatches bind.
pub(super) fn split(
    frame: &OffscreenFrame,
    records: &[Record],
    run: Range<usize>,
    budget: usize,
) -> Vec<Range<usize>> {
    let mut parts = Vec::new();
    let mut start = run.start;
    let mut held = 0_usize;
    for (index, record) in records.iter().enumerate().take(run.end).skip(run.start) {
        let bound = frame.planned_draw_bound(record);
        if held > 0 && held.saturating_add(bound) > budget {
            parts.push(start..index);
            start = index;
            held = 0;
        }
        held = held.saturating_add(bound);
    }
    parts.push(start..run.end);
    parts
}
