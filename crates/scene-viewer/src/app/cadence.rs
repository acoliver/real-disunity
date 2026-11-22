use crate::PresentationCadence;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

pub(super) fn with_wakeup(flow: ControlFlow, deadline: Option<Instant>) -> ControlFlow {
    match (flow, deadline) {
        (ControlFlow::Wait, Some(deadline)) => ControlFlow::WaitUntil(deadline),
        (ControlFlow::WaitUntil(next), Some(deadline)) => {
            ControlFlow::WaitUntil(next.min(deadline))
        }
        _ => flow,
    }
}

/// How soon a due frame is tried again while every frame slot is still in flight. Completions are
/// only seen when the loop polls the device, so a frame waiting for the GPU polls at this interval
/// instead of sleeping until the following deadline.
const RETRY: Duration = Duration::from_millis(1);

/// When the next frame starts. A fixed cadence starts frames one interval apart, counted from
/// each frame's deadline rather than from when it was woken, so late wakeups do not add up. A
/// frame still running when its successor is due makes the successor start as soon as it
/// finishes, and the schedule continues from that start without catching up missed frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Deadline(Instant);

impl Deadline {
    pub(super) fn new(now: Instant) -> Self {
        Self(now)
    }

    fn start(&mut self, now: Instant, interval: Duration) {
        let next = self.0 + interval;
        self.0 = if next > now { next } else { now + interval };
    }

    /// The frame's work ended at `now`.
    pub(super) fn finish(&mut self, now: Instant) {
        self.0 = self.0.max(now);
    }
}

/// Returns whether a frame starts at `now`, and how long the loop may sleep after it is requested.
/// A due frame starts only when `slot_free` says the renderer would admit it; otherwise the
/// deadline stays due and the loop polls again after [`RETRY`].
pub(super) fn schedule(
    cadence: PresentationCadence,
    now: Instant,
    deadline: &mut Deadline,
    active: bool,
    pending: bool,
    slot_free: bool,
) -> (bool, ControlFlow) {
    let completion = now + Duration::from_millis(16);
    if !active {
        return (
            false,
            if pending {
                ControlFlow::WaitUntil(completion)
            } else {
                ControlFlow::Wait
            },
        );
    }
    match cadence {
        PresentationCadence::Uncapped => (true, ControlFlow::Poll),
        PresentationCadence::Fixed(interval) => {
            let due = now >= deadline.0;
            let start = due && slot_free;
            if start {
                deadline.start(now, interval);
            }
            let next = if due && !slot_free {
                now + RETRY
            } else {
                deadline.0
            };
            (
                start,
                ControlFlow::WaitUntil(if pending { next.min(completion) } else { next }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    /// Runs `works.len()` frames of a fixed 16 ms cadence on a clock the test controls. Each wakeup
    /// arrives `late` after the time the loop asked for. Returns the frame start times in
    /// milliseconds from the first start, and for each start whether the loop slept before it.
    fn fly(works: &[u64], late: Duration) -> (Vec<u64>, Vec<bool>) {
        let cadence = PresentationCadence::Fixed(16 * MS);
        let origin = Instant::now();
        let mut now = origin;
        let mut deadline = Deadline::new(now);
        let mut starts = Vec::new();
        let mut slept = Vec::new();
        let mut asleep = false;
        while starts.len() < works.len() {
            match schedule(cadence, now, &mut deadline, true, false, true) {
                (true, ControlFlow::WaitUntil(_)) => {
                    starts.push(u64::try_from((now - origin).as_millis()).unwrap());
                    slept.push(asleep);
                    asleep = false;
                    now += Duration::from_millis(works[starts.len() - 1]);
                    deadline.finish(now);
                }
                (false, ControlFlow::WaitUntil(wake)) => {
                    assert!(wake > now, "a sleep must end in the future");
                    asleep = true;
                    now = wake + late;
                }
                other => panic!("unexpected schedule {other:?}"),
            }
        }
        (starts, slept)
    }

    fn intervals(starts: &[u64]) -> Vec<u64> {
        starts.windows(2).map(|pair| pair[1] - pair[0]).collect()
    }

    #[test]
    fn short_frames_start_one_interval_apart_counted_from_their_starts() {
        let (starts, slept) = fly(&[5; 8], Duration::ZERO);
        assert_eq!(intervals(&starts), vec![16; 7]);
        assert_eq!(slept[1..], [true; 7]);
    }

    #[test]
    fn late_wakeups_do_not_push_the_schedule_back() {
        let (starts, _) = fly(&[5; 8], MS);
        assert_eq!(starts, vec![0, 17, 33, 49, 65, 81, 97, 113]);
    }

    #[test]
    fn frames_longer_than_the_interval_follow_each_other_without_sleeping() {
        let (starts, slept) = fly(&[25; 8], Duration::ZERO);
        assert_eq!(intervals(&starts), vec![25; 7]);
        assert_eq!(slept, vec![false; 8]);
    }

    #[test]
    fn an_overrun_restarts_the_schedule_from_the_next_start_without_catching_up() {
        let (starts, slept) = fly(&[5, 100, 5, 5, 25, 5, 5], Duration::ZERO);
        assert_eq!(starts, vec![0, 16, 116, 132, 148, 173, 189]);
        assert_eq!(slept, vec![false, true, false, true, true, false, true]);
    }

    /// Runs `frames` frames of a fixed 16 ms cadence with two frame slots: each frame takes `cpu`
    /// ms on the loop, then `gpu` ms on a GPU that runs one frame at a time, and its slot frees
    /// when its GPU work completes. Returns the frame start times in milliseconds.
    fn fly_gpu(cpu: u64, gpu: u64, frames: usize) -> Vec<u64> {
        let cadence = PresentationCadence::Fixed(16 * MS);
        let origin = Instant::now();
        let mut now = origin;
        let mut deadline = Deadline::new(now);
        let mut in_flight = std::collections::VecDeque::new();
        let mut gpu_idle = origin;
        let mut starts = Vec::new();
        while starts.len() < frames {
            in_flight.retain(|done| *done > now);
            match schedule(
                cadence,
                now,
                &mut deadline,
                true,
                false,
                in_flight.len() < 2,
            ) {
                (true, ControlFlow::WaitUntil(_)) => {
                    starts.push(u64::try_from((now - origin).as_millis()).unwrap());
                    now += Duration::from_millis(cpu);
                    gpu_idle = gpu_idle.max(now) + Duration::from_millis(gpu);
                    in_flight.push_back(gpu_idle);
                    deadline.finish(now);
                }
                (false, ControlFlow::WaitUntil(wake)) => {
                    assert!(wake > now, "a sleep must end in the future");
                    now = wake;
                }
                other => panic!("unexpected schedule {other:?}"),
            }
        }
        starts
    }

    #[test]
    fn a_frame_waiting_for_a_slot_starts_within_a_millisecond_of_the_gpu_freeing_it() {
        let starts = fly_gpu(22, 30, 10);
        assert_eq!(starts[..3], [0, 22, 52]);
        assert_eq!(intervals(&starts[2..]), vec![30; 7]);
    }

    #[test]
    fn a_due_frame_without_a_free_slot_keeps_its_deadline_and_polls_again_soon() {
        let cadence = PresentationCadence::Fixed(16 * MS);
        let now = Instant::now();
        let mut deadline = Deadline::new(now);
        assert_eq!(
            schedule(cadence, now, &mut deadline, true, false, false),
            (false, ControlFlow::WaitUntil(now + RETRY))
        );
        assert_eq!(deadline, Deadline::new(now));
        assert_eq!(
            schedule(cadence, now, &mut deadline, true, true, false),
            (false, ControlFlow::WaitUntil(now + RETRY))
        );
        let freed = now + 3 * RETRY;
        assert_eq!(
            schedule(cadence, freed, &mut deadline, true, false, true),
            (true, ControlFlow::WaitUntil(now + 16 * MS))
        );
    }

    #[test]
    fn a_stale_deadline_after_a_pause_starts_one_frame_then_waits_a_full_interval() {
        let cadence = PresentationCadence::Fixed(16 * MS);
        let start = Instant::now();
        let mut deadline = Deadline::new(start);
        let resumed = start + 10 * Duration::from_secs(1);
        assert_eq!(
            schedule(cadence, resumed, &mut deadline, true, false, true),
            (true, ControlFlow::WaitUntil(resumed + 16 * MS))
        );
        deadline.finish(resumed + 5 * MS);
        assert_eq!(
            schedule(cadence, resumed + 5 * MS, &mut deadline, true, false, true),
            (false, ControlFlow::WaitUntil(resumed + 16 * MS))
        );
    }

    #[test]
    fn timers_wake_paused_loops_and_preserve_earlier_frame_or_completion_deadlines() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        assert_eq!(
            with_wakeup(ControlFlow::Wait, Some(now)),
            ControlFlow::WaitUntil(now)
        );
        assert_eq!(
            with_wakeup(ControlFlow::WaitUntil(later), Some(now)),
            ControlFlow::WaitUntil(now)
        );
        assert_eq!(
            with_wakeup(ControlFlow::WaitUntil(now), Some(later)),
            ControlFlow::WaitUntil(now)
        );
        assert_eq!(with_wakeup(ControlFlow::Poll, Some(now)), ControlFlow::Poll);
        assert_eq!(with_wakeup(ControlFlow::Wait, None), ControlFlow::Wait);
        let (_, paused) = schedule(
            PresentationCadence::default(),
            now,
            &mut Deadline::new(later),
            false,
            false,
            true,
        );
        assert_eq!(
            with_wakeup(paused, Some(later)),
            ControlFlow::WaitUntil(later)
        );
    }

    #[test]
    fn fixed_cadence_obeys_deadline_and_capture_polling() {
        let now = Instant::now();
        let cadence = PresentationCadence::Fixed(100 * MS);
        let next = now + 100 * MS;
        let mut deadline = Deadline::new(next);
        assert_eq!(
            schedule(cadence, now, &mut deadline, true, false, true),
            (false, ControlFlow::WaitUntil(next))
        );
        assert_eq!(
            schedule(cadence, now, &mut deadline, true, true, true),
            (false, ControlFlow::WaitUntil(now + 16 * MS))
        );
        assert_eq!(
            schedule(cadence, next, &mut deadline, true, false, true),
            (true, ControlFlow::WaitUntil(next + 100 * MS))
        );
        assert_eq!(deadline, Deadline::new(next + 100 * MS));
        assert_eq!(
            PresentationCadence::default(),
            PresentationCadence::Fixed(16 * MS)
        );
    }

    #[test]
    fn uncapped_presents_on_input_wake_but_pauses_when_inactive() {
        let now = Instant::now();
        for pending in [false, true] {
            assert_eq!(
                schedule(
                    PresentationCadence::Uncapped,
                    now,
                    &mut Deadline::new(now),
                    true,
                    pending,
                    false
                ),
                (true, ControlFlow::Poll)
            );
        }
        for cadence in [
            PresentationCadence::default(),
            PresentationCadence::Uncapped,
        ] {
            assert_eq!(
                schedule(cadence, now, &mut Deadline::new(now), false, false, true),
                (false, ControlFlow::Wait)
            );
            assert_eq!(
                schedule(cadence, now, &mut Deadline::new(now), false, true, true),
                (false, ControlFlow::WaitUntil(now + 16 * MS))
            );
        }
    }
}
