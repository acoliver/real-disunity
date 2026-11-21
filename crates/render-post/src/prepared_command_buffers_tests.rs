//! What compute work staged on the frame's preparation costs in command buffers. wgpu-core
//! encodes each compute pass into two, a copy after a pass reopens one, and Metal refuses the
//! command buffer after its 4096 created and not submitted as a lost device, so the largest amount
//! of work one frame of a fresh device submits counts them ([`largest_submitting_count`]). Other
//! backends have no such cap: every count submits there, and the tests check only that the frames
//! submit.

use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_gpu::test_support::largest_submitting_count;

use crate::prepared_test_support::{BufferCopy, PassDispatch, Recorded, pipeline};
use crate::{FrameError, PrepareContext, PreparedAccess};

/// More filler passes than Metal's 4096 command buffers hold at two each.
const FILLERS: usize = 2049;
/// More pages than Metal's 4096 command buffers hold at the 12 a batched page costs.
const PAGES: usize = 400;
/// Dispatches staged one after another.
const RUN: usize = 256;

const COUNTER: &str = "
@group(0) @binding(0) var<storage, read_write> count: array<u32>;
@compute @workgroup_size(1) fn main() { count[0] = count[0] + 1u; }
";

/// A counting kernel and its bind group, and two buffers to copy between.
struct Counter {
    pipeline: wgpu::ComputePipeline,
    group: wgpu::BindGroup,
    source: wgpu::Buffer,
    destination: wgpu::Buffer,
}

impl Counter {
    fn new(device: &wgpu::Device) -> Self {
        let pipeline = pipeline(device, "count", COUNTER);
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 16,
                usage,
                mapped_at_creation: false,
            })
        };
        let count = buffer("count", wgpu::BufferUsages::STORAGE);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("count"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: count.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            group,
            source: buffer("copied", wgpu::BufferUsages::COPY_SRC),
            destination: buffer("copy", wgpu::BufferUsages::COPY_DST),
        }
    }

    /// A compute pass of its own, recorded on `encoder`, with `dispatches` dispatches.
    fn raw_pass(&self, encoder: &mut wgpu::CommandEncoder, dispatches: usize) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        for _ in 0..dispatches {
            pass.dispatch_workgroups(1, 1, 1);
        }
    }

    fn dispatch(&self) -> PassDispatch {
        PassDispatch::new(&self.pipeline, &self.group)
    }

    fn copy(&self) -> BufferCopy {
        BufferCopy {
            source: self.source.clone(),
            destination: self.destination.clone(),
            bytes: 16,
        }
    }
}

/// Records one frame with `record` on a fresh device and submits it; the errors the device
/// reported when it refused the frame.
fn submit(
    record: impl FnOnce(&wgpu::Device, &Counter, &mut wgpu::CommandEncoder) -> Result<(), FrameError>,
) -> Result<(), Vec<String>> {
    let recorded = Recorded::new();
    let counter = Counter::new(&recorded.device);
    let prepared = SubmissionAuthority::new(recorded.queue.clone())
        .prepare(&recorded.device, |encoder, _| {
            record(&recorded.device, &counter, encoder)
        });
    match prepared {
        Ok(frame) => {
            if let Err(error) = frame.submit() {
                recorded.report(format!("submit: {error}"));
            }
        }
        Err(error) => recorded.report(format!("prepare: {error:?}")),
    }
    recorded.outcome()
}

/// Stages `units` on a preparation of `encoder` and finishes it.
fn prepare(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    units: impl FnOnce(&mut PrepareContext<'_>) -> Result<(), FrameError>,
) -> Result<(), FrameError> {
    let mut context = PrepareContext::new(device, encoder)?;
    context.open_preparation();
    units(&mut context)?;
    context.finish_preparation()?;
    Ok(())
}

/// The largest count in `0..=cap` whose frame submits; a refusal must be the command buffer cap.
fn largest(name: &str, cap: usize, submits: impl FnMut(usize) -> Result<(), Vec<String>>) -> usize {
    let (largest, refusal) = largest_submitting_count(cap, submits);
    if let Some(refusal) = &refusal {
        assert!(
            refusal.iter().any(|error| error.starts_with("lost:")),
            "{name}: the frame after {largest} was refused for another reason than the command \
             buffer cap: {refusal:?}"
        );
    }
    eprintln!("MEASURE {name}: largest {largest} of {cap}, refusal {refusal:?}");
    largest
}

#[derive(Clone, Copy, Debug)]
enum Run {
    /// One compute pass of [`RUN`] dispatches, recorded on the frame's encoder.
    OnePass,
    /// [`RUN`] prepared dispatches staged one after another.
    Prepared,
    /// [`RUN`] dispatches staged one after another, alternately prepared and through
    /// [`PrepareContext::dispatch`].
    Alternating,
}

/// Submits a frame of `fillers` compute passes of one dispatch each, recorded on its encoder
/// before its preparation, then `run`.
fn submit_beside_fillers(run: Run, fillers: usize) -> Result<(), Vec<String>> {
    submit(|device, counter, encoder| {
        for _ in 0..fillers {
            counter.raw_pass(encoder, 1);
        }
        if matches!(run, Run::OnePass) {
            counter.raw_pass(encoder, RUN);
            return Ok(());
        }
        prepare(device, encoder, |context| {
            for index in 0..RUN {
                if matches!(run, Run::Alternating) && index % 2 == 1 {
                    context.dispatch("count", &counter.pipeline, &counter.group, [1, 1, 1]);
                } else {
                    context.record_prepared(
                        "count",
                        PreparedAccess::default(),
                        Box::new(counter.dispatch()),
                    )?;
                }
            }
            Ok(())
        })
    })
}

/// A run of dispatches staged one after another costs what one compute pass costs.
///
/// Fillers measure the run's cost: with filler passes of 2 command buffers each, the fixed cost
/// `c` of a frame and work costing `w`, the largest number of fillers is
/// `floor((4096 - c - w) / 2)` on Metal. Equal counts beside a compute pass of [`RUN`] dispatches
/// (`w = 2`) and beside the staged run mean the run costs 1 or 2; one pass per dispatch would cost
/// `2 * RUN` and leave room for `RUN - 1` fewer fillers.
#[test]
fn consecutive_prepared_dispatches_cost_one_compute_pass() {
    let one_pass = largest("fillers beside one pass", FILLERS, |fillers| {
        submit_beside_fillers(Run::OnePass, fillers)
    });
    for run in [Run::Prepared, Run::Alternating] {
        let staged = largest(&format!("fillers beside {run:?}"), FILLERS, |fillers| {
            submit_beside_fillers(run, fillers)
        });
        assert_eq!(
            staged, one_pass,
            "one frame takes {one_pass} filler passes beside a compute pass of {RUN} dispatches \
             but {staged} beside {RUN} {run:?} dispatches staged one after another"
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Page {
    /// Staged on the frame's preparation.
    Staged,
    /// Recorded on the frame's encoder as one compute pass for each run of dispatches, with the
    /// transfers' copies between them.
    Batched,
}

/// Submits a frame of `pages` pages. A page is staged as a game's terrain page is: 12 dispatches,
/// then for each of 4 mips 3 dispatches and 5 transfers, so its runs of dispatches are 15, 3, 3
/// and 3 long.
fn submit_pages(page: Page, pages: usize) -> Result<(), Vec<String>> {
    let runs = [15, 3, 3, 3];
    submit(|device, counter, encoder| match page {
        Page::Staged => prepare(device, encoder, |context| {
            for _ in 0..pages {
                for dispatches in runs {
                    for _ in 0..dispatches {
                        context.record_prepared(
                            "dispatch",
                            PreparedAccess::default(),
                            Box::new(counter.dispatch()),
                        )?;
                    }
                    for _ in 0..5 {
                        context.record_prepared(
                            "transfer",
                            PreparedAccess::default(),
                            Box::new(counter.copy()),
                        )?;
                    }
                }
            }
            Ok(())
        }),
        Page::Batched => {
            for _ in 0..pages {
                for dispatches in runs {
                    counter.raw_pass(encoder, dispatches);
                    for _ in 0..5 {
                        crate::PreparedCommands::record(&counter.copy(), encoder, "copy").unwrap();
                    }
                }
            }
            Ok(())
        }
    })
}

/// A transfer ends a run of dispatches, and a page of runs and transfers costs its runs' passes
/// and the command buffers its transfers reopen after them: 4 x 2 + 4 = 12 on Metal, so 341 pages
/// fit one frame (12 x 341 + 2 <= 4096 < 12 x 342 + 2).
#[test]
fn a_page_of_runs_and_transfers_costs_its_runs_and_transfers() {
    let batched = largest("pages batched by hand", PAGES, |pages| {
        submit_pages(Page::Batched, pages)
    });
    let staged = largest("pages staged", PAGES, |pages| {
        submit_pages(Page::Staged, pages)
    });
    assert_eq!(
        staged, batched,
        "one frame submits {batched} pages recorded as one pass per run but {staged} staged"
    );
}
