//! Opt-in, bounded CPU wall spans for a live viewer on the calling thread.

use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

thread_local! {
    static ACTIVE: RefCell<Option<Records>> = const { RefCell::new(None) };
}

struct Records {
    rows: Vec<CpuSpan>,
    frame: u64,
    dropped: u64,
}

/// One CPU wall measurement. Nested spans overlap and must not be summed.
#[derive(Debug)]
pub struct CpuSpan {
    /// Player update identity, or zero during renderer initialization.
    pub frame: u64,
    /// Stable measured boundary.
    pub stage: &'static str,
    /// Monotonic CPU wall duration, including waits inside the boundary.
    pub duration: Duration,
    /// Optional counts or actual adapter identity.
    pub detail: Option<String>,
}

/// Owns a recorder on this thread. Dropping it discards uncollected records.
pub struct CpuProfile(PhantomData<Rc<()>>);

impl CpuProfile {
    /// Start collecting at most 4096 rows. Nested profiling is rejected.
    pub fn begin() -> Option<Self> {
        ACTIVE.with_borrow_mut(|active| {
            if active.is_some() {
                return None;
            }
            *active = Some(Records {
                rows: Vec::new(),
                frame: 0,
                dropped: 0,
            });
            Some(Self(PhantomData))
        })
    }

    /// Correlate subsequent spans with the supplied update identity.
    pub fn frame(frame: u64) {
        ACTIVE.with_borrow_mut(|active| {
            if let Some(records) = active {
                records.frame = frame;
            }
        });
    }

    /// Start a CPU span; inactive profiling does not read the clock.
    pub fn span(stage: &'static str) -> Span {
        let start = ACTIVE.with_borrow(|active| {
            active
                .as_ref()
                .map(|records| (Instant::now(), records.frame))
        });
        Span {
            stage,
            start,
            thread: PhantomData,
        }
    }

    /// Add untimed metadata only when recording is active.
    pub fn detail(stage: &'static str, detail: impl FnOnce() -> String) {
        ACTIVE.with_borrow_mut(|active| {
            if let Some(records) = active {
                records.push(CpuSpan {
                    frame: records.frame,
                    stage,
                    duration: Duration::ZERO,
                    detail: Some(detail()),
                });
            }
        });
    }

    /// Stop collection and return retained rows and the count omitted at the bound.
    pub fn finish(self) -> (Vec<CpuSpan>, u64) {
        ACTIVE.with_borrow_mut(|active| {
            let records = active.take().expect("owned CPU recorder");
            (records.rows, records.dropped)
        })
    }
}

impl Drop for CpuProfile {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| {
            active.take();
        });
    }
}

impl Records {
    fn push(&mut self, row: CpuSpan) {
        if self.rows.len() < 4096 {
            self.rows.push(row);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}

/// Records the measured boundary on drop, including error exits.
pub struct Span {
    stage: &'static str,
    start: Option<(Instant, u64)>,
    thread: PhantomData<Rc<()>>,
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some((start, frame)) = self.start {
            let duration = start.elapsed();
            ACTIVE.with_borrow_mut(|active| {
                if let Some(records) = active {
                    records.push(CpuSpan {
                        frame,
                        stage: self.stage,
                        duration,
                        detail: None,
                    });
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_are_opt_in_correlated_and_bounded() {
        drop(CpuProfile::span("inactive"));
        let profile = CpuProfile::begin().unwrap();
        assert!(CpuProfile::begin().is_none());
        CpuProfile::frame(7);
        {
            let _span = CpuProfile::span("native_advance");
        }
        CpuProfile::detail("adapter", || "selected adapter".into());
        for _ in 0..4096 {
            drop(CpuProfile::span("overflow"));
        }
        let (rows, dropped) = profile.finish();
        assert_eq!(rows.len(), 4096);
        assert_eq!(dropped, 2);
        assert_eq!(rows[0].frame, 7);
        assert_eq!(rows[0].stage, "native_advance");
        assert!(
            rows[0]
                .duration
                .as_nanos()
                .to_string()
                .parse::<u128>()
                .is_ok()
        );
        assert_eq!(rows[1].detail.as_deref(), Some("selected adapter"));
        assert!(CpuProfile::begin().is_some());
    }
}
