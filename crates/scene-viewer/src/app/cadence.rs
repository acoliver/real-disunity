use crate::PresentationCadence;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

pub(super) fn next_frame(now: Instant, cadence: PresentationCadence) -> Instant {
    match cadence {
        PresentationCadence::Fixed(interval) => now + interval,
        PresentationCadence::Uncapped => now,
    }
}

pub(super) fn schedule(
    cadence: PresentationCadence,
    now: Instant,
    next: Instant,
    active: bool,
    pending: bool,
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
        PresentationCadence::Fixed(_) => {
            let due = now >= next;
            let next = if due { next_frame(now, cadence) } else { next };
            (
                due,
                ControlFlow::WaitUntil(if pending { next.min(completion) } else { next }),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_cadence_obeys_deadline_and_capture_polling() {
        let now = Instant::now();
        let cadence = PresentationCadence::Fixed(Duration::from_millis(100));
        let next = next_frame(now, cadence);
        assert_eq!(
            schedule(cadence, now, next, true, false),
            (false, ControlFlow::WaitUntil(next))
        );
        assert_eq!(
            schedule(cadence, now, next, true, true),
            (
                false,
                ControlFlow::WaitUntil(now + Duration::from_millis(16))
            )
        );
        assert_eq!(
            schedule(cadence, next, next, true, false),
            (true, ControlFlow::WaitUntil(next_frame(next, cadence)))
        );
        assert_eq!(
            PresentationCadence::default(),
            PresentationCadence::Fixed(Duration::from_millis(16))
        );
    }

    #[test]
    fn uncapped_presents_on_input_wake_but_pauses_when_inactive() {
        let now = Instant::now();
        for pending in [false, true] {
            assert_eq!(
                schedule(PresentationCadence::Uncapped, now, now, true, pending),
                (true, ControlFlow::Poll)
            );
        }
        for cadence in [
            PresentationCadence::default(),
            PresentationCadence::Uncapped,
        ] {
            assert_eq!(
                schedule(cadence, now, now, false, false),
                (false, ControlFlow::Wait)
            );
            assert_eq!(
                schedule(cadence, now, now, false, true),
                (
                    false,
                    ControlFlow::WaitUntil(now + Duration::from_millis(16))
                )
            );
        }
    }
}
