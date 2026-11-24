//! Preparing a frame for `--read-target`: what the frame's records are, and where the reads go.
//!
//! The index of a record is its place in the frame's adapted records, counting every record
//! including those of globals. The numbers are printed for every record of a frame that has a
//! read, before the frame is built, so that a run can name the ones it wants.

use worldrunner_render_program::unity::offscreen::frame::{Record, RenderTextureSpec};

use super::frame::Adapted;
use crate::TargetRead;

/// What a record is in a listing and in the line printed for a read: its kind and its target.
fn describe(record: &Record) -> String {
    match record.target() {
        Some(target) => format!("{} target #{:x}", record.kind(), target.0),
        None => record.kind().to_owned(),
    }
}

/// The size, format and depth bits of a render texture, as a listing prints them.
fn shape(spec: &RenderTextureSpec) -> String {
    format!(
        "{}x{} format {} depth {}",
        spec.size[0], spec.size[1], spec.graphics_format, spec.depth_bits
    )
}

/// The line that names record `index` of frame `frame`: its kind, its target and the target's
/// shape, and the depth attachment it renders with when that is another texture than the target.
pub(super) fn listing(
    frame: u64,
    index: usize,
    record: &Record,
    textures: &[RenderTextureSpec],
) -> String {
    let find = |id| textures.iter().find(|spec| spec.id == id);
    let target = record.target();
    let size = target
        .and_then(find)
        .map(|spec| format!(" {}", shape(spec)))
        .unwrap_or_default();
    let depth = match (target, record.depth_target()) {
        (Some(colour), Some(depth)) if depth != colour => {
            let shape = find(depth).map(|spec| format!(" {}", shape(spec)));
            format!(
                " depth attachment #{:x}{}",
                depth.0,
                shape.unwrap_or_default()
            )
        }
        _ => String::new(),
    };
    format!(
        "[gpu] frame {frame} record {index} {}{size}{depth}",
        describe(record)
    )
}

/// Lists the records of the frame `reads` name and asks the frame for a read after each record
/// they name. The listing goes to standard error and keeps the numbers the reads use, which the
/// build may change by leaving records out. `reads` are all of one frame.
pub(super) fn arrange(adapted: &mut Adapted, reads: &[TargetRead]) {
    let Some(first) = reads.first() else {
        return;
    };
    for (index, record) in adapted.frame.records.iter().enumerate() {
        eprintln!(
            "{}",
            listing(first.frame, index, record, &adapted.frame.render_textures)
        );
    }
    adapted.frame.taps = reads.iter().map(|read| read.record).collect();
    adapted.report.reads = reads
        .iter()
        .filter_map(|read| {
            let record = adapted.frame.records.get(read.record)?;
            Some((read.record, describe(record)))
        })
        .collect();
}
