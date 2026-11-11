//! Where a flush or a frame of offscreen work spent its time and what it held, for the run's
//! report. Counts and times only; nothing of the game's data leaves the decoders.

use std::fmt::Write as _;
use std::time::Duration;

/// The time a flush or frame spent in each of its steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Phases {
    /// Loading and uploading what the snapshot names and mapping it onto a frame.
    pub adapt: Duration,
    /// Planning the draws, packing their constants and choosing their programs
    /// (`OffscreenState::build`), including the builds that followed a refused frame.
    pub build: Duration,
    /// Of `build`, the time the draws spent choosing and building their programs.
    pub programs: Duration,
    /// Recording the operations and the dispatches, submitting them and waiting.
    pub submit: Duration,
    /// Times the whole frame was refused and its render textures and records were built one at a
    /// time to find the ones the state refuses.
    pub refusals: usize,
    /// The builds of one render texture or one record alone that followed.
    pub probes: usize,
    /// Programs the draws built, and the bytes of constants the kept draws upload.
    pub programs_built: usize,
    pub constants_bytes: u64,
    /// The buffers the constants went into, the most bytes one held, the blocks packed and the
    /// blocks that shared an earlier slice.
    pub constants_buffers: usize,
    pub constants_largest: u64,
    pub constants_blocks: usize,
    pub constants_shared: usize,
}

impl Phases {
    /// Adds `other`'s times and counts to these.
    pub fn add(&mut self, other: &Self) {
        self.adapt += other.adapt;
        self.build += other.build;
        self.programs += other.programs;
        self.submit += other.submit;
        self.refusals += other.refusals;
        self.probes += other.probes;
        self.programs_built += other.programs_built;
        self.constants_bytes += other.constants_bytes;
        self.constants_buffers += other.constants_buffers;
        self.constants_largest = self.constants_largest.max(other.constants_largest);
        self.constants_blocks += other.constants_blocks;
        self.constants_shared += other.constants_shared;
    }
}

/// One flush or frame as the run's report lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkTrace {
    /// `flush` or `frame`.
    pub kind: &'static str,
    /// The records the world listed and the frame records built from them.
    pub records: usize,
    pub built: usize,
    /// Camera renders, the draws they recorded and the draws they left out for any cause.
    pub camera_renders: usize,
    pub drawn: usize,
    pub left_out: usize,
    /// Bytes the runtime held before and after.
    pub live_before: u64,
    pub live_after: u64,
    pub phases: Phases,
    /// Why the work was refused, if it was.
    pub error: Option<String>,
}

impl WorkTrace {
    /// The trace as a report line.
    #[must_use]
    pub fn line(&self, index: usize) -> String {
        let seconds = |time: Duration| time.as_secs_f64();
        let mut line = format!(
            "{} {index}: records {}, built {}, camera renders {} drew {} left out {}, programs \
             built {}, constants {} bytes in {} buffers (largest {}, {} blocks, {} shared), held {} then {} \
             bytes, adapt {:.1} s, build {:.1} s \
             (programs {:.1} s), submit {:.1} s, whole-frame refusals {} with {} builds alone",
            self.kind,
            self.records,
            self.built,
            self.camera_renders,
            self.drawn,
            self.left_out,
            self.phases.programs_built,
            self.phases.constants_bytes,
            self.phases.constants_buffers,
            self.phases.constants_largest,
            self.phases.constants_blocks,
            self.phases.constants_shared,
            self.live_before,
            self.live_after,
            seconds(self.phases.adapt),
            seconds(self.phases.build),
            seconds(self.phases.programs),
            seconds(self.phases.submit),
            self.phases.refusals,
            self.phases.probes,
        );
        if let Some(error) = &self.error {
            write!(line, ", refused: {error}").ok();
        }
        line
    }
}
