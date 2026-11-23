//! Reads of a record's target in the middle of a frame: right after the record is planned, the
//! target's colour and depth are copied into staging buffers, so the copy sees what the records
//! up to that one drew and nothing after.

use crate::ids::TextureId;

use super::frame::{OffscreenFrame, Record};
use super::plan::Services;
use super::readback::{PendingImage, ReadSource, TapRead, readback_operation};
use super::steps::{Step, raster_id};
use super::tracker::Imported;

/// The read the frame asks for after record `after`, if it asks for one: the copies go into
/// `steps` right after the record's own steps, so they see what the records up to it drew.
pub(super) fn plan_taps(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    after: usize,
    steps: &mut Vec<Step>,
) -> Option<TapRead> {
    let record = frame.records.get(after)?;
    frame
        .taps
        .contains(&after)
        .then(|| tap_record(svc, record, after, steps))
}

/// The refusals for the taps that name a record past the end of the frame.
pub(super) fn past_the_end(frame: &OffscreenFrame) -> Vec<TapRead> {
    let mut past: Vec<usize> = frame
        .taps
        .iter()
        .copied()
        .filter(|tap| *tap >= frame.records.len())
        .collect();
    past.sort_unstable();
    past.dedup();
    past.into_iter()
        .map(|record| {
            let why = format!(
                "record {record} is past the {} records of the frame",
                frame.records.len()
            );
            refused(record, &why)
        })
        .collect()
}

fn refused(record: usize, why: &str) -> TapRead {
    TapRead {
        record,
        colour: Err(why.to_owned()),
        depth: Err(why.to_owned()),
    }
}

fn tap_record(
    svc: &mut Services<'_>,
    record: &Record,
    index: usize,
    steps: &mut Vec<Step>,
) -> TapRead {
    let Some(target) = record.target() else {
        let why = format!(
            "record {index} is a {} record, which draws nothing",
            record.kind()
        );
        return refused(index, &why);
    };
    let imported = match svc.tracker.imported(target) {
        Some(imported) => Ok(imported),
        None => svc.import(target).map(|(imported, _)| imported),
    };
    let imported = match imported {
        Ok(imported) => imported,
        Err(error) => return refused(index, &format!("record {index}: {error}")),
    };
    let mut read = |colour: bool| {
        let source = source_of(target, imported, colour)?;
        let id = raster_id("offscreen tap", index * 2 + usize::from(!colour));
        let (operation, pending) =
            readback_operation(svc.runtime.device(), &mut *svc.tracker, &source, id)
                .map_err(|error| format!("record {index} target {target:?}: {error}"))?;
        steps.push(Step::Ready(operation));
        Ok::<PendingImage, String>(pending)
    };
    let colour = read(true);
    let depth = read(false);
    TapRead {
        record: index,
        colour,
        depth,
    }
}

/// The first mip and layer of the colour or the depth of `imported`, which a target may lack.
fn source_of(texture: TextureId, imported: Imported, colour: bool) -> Result<ReadSource, String> {
    let (key, format, aspect, what) = if colour {
        (
            imported.color,
            Some(imported.format),
            wgpu::TextureAspect::All,
            "no colour",
        )
    } else {
        (
            imported.depth,
            imported.depth_format,
            wgpu::TextureAspect::DepthOnly,
            "no depth buffer",
        )
    };
    let (Some(key), Some(format)) = (key, format) else {
        return Err(format!("target {texture:?} has {what}"));
    };
    Ok(ReadSource {
        texture,
        key,
        format,
        size: imported.size,
        aspect,
        mip: 0,
        layer: 0,
    })
}
