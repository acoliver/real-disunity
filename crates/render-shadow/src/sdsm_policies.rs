//! Explicit alternatives to practical splits and frame-indexed contraction.
use crate::{DepthRange, InvalidFit};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SplitPolicy {
    #[default]
    Practical,
    OverlapRecurrence {
        multiplier: f32,
        overlap: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CascadeInterval {
    pub start: f32,
    pub end: f32,
    pub fade: f32,
}
impl SplitPolicy {
    pub fn validate(self) -> Result<(), InvalidFit> {
        if let Self::OverlapRecurrence {
            multiplier,
            overlap,
        } = self
            && (!multiplier.is_finite()
                || multiplier < 1.0
                || !overlap.is_finite()
                || !(0.0..1.0).contains(&overlap))
        {
            return Err(InvalidFit);
        }
        Ok(())
    }
    pub fn intervals(
        self,
        near: f32,
        far: f32,
        count: u16,
    ) -> Result<Vec<CascadeInterval>, InvalidFit> {
        self.validate()?;
        let Self::OverlapRecurrence {
            multiplier,
            overlap,
        } = self
        else {
            return Err(InvalidFit);
        };
        if count == 0
            || count > 4
            || !near.is_finite()
            || !far.is_finite()
            || near <= 0.0
            || far <= near
        {
            return Err(InvalidFit);
        }
        let mut width = 1.0;
        let mut fade = overlap;
        let mut start = 0.0;
        let mut intervals = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            intervals.push(CascadeInterval {
                start,
                end: start + width,
                fade,
            });
            start += width - fade;
            width *= multiplier;
            fade = (width - fade) * overlap;
        }
        let scale = (far - near) / intervals.last().expect("nonzero count").end;
        for interval in &mut intervals {
            interval.start = near + interval.start * scale;
            interval.end = near + interval.end * scale;
            interval.fade *= scale;
            if !interval.start.is_finite()
                || !interval.end.is_finite()
                || !interval.fade.is_finite()
                || interval.end <= interval.start
            {
                return Err(InvalidFit);
            }
        }
        intervals.last_mut().expect("nonzero count").end = far;
        Ok(intervals)
    }
}

/// Each bound waits independently from the start of a continuous contraction.
/// An expansion, equality or empty coverage cancels that bound's pending timer.
#[derive(Clone, Copy, Debug, Default)]
pub struct PerBoundHysteresis {
    current: Option<DepthRange>,
    near_since: Option<Duration>,
    far_since: Option<Duration>,
    last_time: Option<Duration>,
}
impl PerBoundHysteresis {
    pub fn update(
        &mut self,
        candidate: DepthRange,
        elapsed: Duration,
        delay: Duration,
    ) -> Result<DepthRange, InvalidFit> {
        candidate.validate()?;
        if self.last_time.is_some_and(|last| elapsed < last) {
            return Err(InvalidFit);
        }
        self.last_time = Some(elapsed);
        let result = if let (
            Some(DepthRange::Receivers { near, far }),
            DepthRange::Receivers {
                near: next_near,
                far: next_far,
            },
        ) = (self.current, candidate)
        {
            DepthRange::Receivers {
                near: bound(
                    near,
                    next_near,
                    next_near > near,
                    &mut self.near_since,
                    elapsed,
                    delay,
                ),
                far: bound(
                    far,
                    next_far,
                    next_far < far,
                    &mut self.far_since,
                    elapsed,
                    delay,
                ),
            }
        } else {
            self.near_since = None;
            self.far_since = None;
            candidate
        };
        self.current = Some(result);
        Ok(result)
    }
}
fn bound(
    current: f32,
    next: f32,
    contracts: bool,
    since: &mut Option<Duration>,
    now: Duration,
    delay: Duration,
) -> f32 {
    if !contracts {
        *since = None;
        return next;
    }
    let start = *since.get_or_insert(now);
    if now.checked_sub(start).expect("monotonic elapsed time") >= delay {
        *since = None;
        next
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recurrence_matches_independent_reference() {
        for multiplier in [1.0, 1.5, 2.0, 4.0] {
            for overlap in [0.0, 0.1, 0.4, 0.8] {
                for count in 1..=4 {
                    let policy = SplitPolicy::OverlapRecurrence {
                        multiplier,
                        overlap,
                    };
                    let intervals = policy.intervals(2.0, 182.0, count).unwrap();
                    let first_width = intervals[0].end - 2.0;
                    let mut start = 2.0;
                    let mut width = first_width;
                    let mut fade = width * overlap;
                    for interval in &intervals {
                        assert!((interval.start - start).abs() < 0.0001);
                        assert!((interval.end - start - width).abs() < 0.0001);
                        assert!((interval.fade - fade).abs() < 0.0001);
                        start += width - fade;
                        width *= multiplier;
                        fade = (width - fade) * overlap;
                    }
                    assert_eq!(intervals.last().unwrap().end.to_bits(), 182.0_f32.to_bits());
                }
            }
        }
        assert!(
            SplitPolicy::OverlapRecurrence {
                multiplier: 0.0,
                overlap: 0.1
            }
            .validate()
            .is_err()
        );
        assert!(
            SplitPolicy::OverlapRecurrence {
                multiplier: 2.0,
                overlap: 1.0
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn bounds_have_independent_elapsed_timers_and_expand_immediately() {
        let mut filter = PerBoundHysteresis::default();
        let r = |near, far| DepthRange::Receivers { near, far };
        let mut update = |near, far, ms| {
            filter
                .update(
                    r(near, far),
                    Duration::from_millis(ms),
                    Duration::from_millis(100),
                )
                .unwrap()
        };
        assert_eq!(update(2.0, 20.0, 0), r(2.0, 20.0));
        assert_eq!(update(3.0, 20.0, 10), r(2.0, 20.0));
        assert_eq!(update(3.0, 18.0, 60), r(2.0, 20.0));
        assert_eq!(update(3.0, 18.0, 110), r(3.0, 20.0));
        assert_eq!(update(1.0, 18.0, 160), r(1.0, 18.0));
        assert_eq!(update(2.0, 30.0, 170), r(1.0, 30.0));
        assert!(
            filter
                .update(r(2.0, 30.0), Duration::ZERO, Duration::ZERO)
                .is_err()
        );
        assert_eq!(
            filter
                .update(
                    DepthRange::EmptyDepth,
                    Duration::from_secs(1),
                    Duration::ZERO
                )
                .unwrap(),
            DepthRange::EmptyDepth
        );
    }
}
