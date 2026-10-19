use super::*;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn callback_collected_by_another_poller_can_finish_after_our_poll() {
    let (send, recv) = mpsc::sync_channel(1);
    let (collected, collected_recv) = mpsc::sync_channel(1);
    let (release, release_recv) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let callback = scope.spawn(move || {
            collected.send(()).unwrap();
            // Model a concurrent poller paused after collecting the callback.
            if release_recv.recv_timeout(TEST_TIMEOUT).is_ok() {
                send.send(Ok(())).unwrap();
            }
        });
        let result = wait_for_map(
            Instant::now() + TEST_TIMEOUT,
            |_| {
                collected_recv.recv_timeout(TEST_TIMEOUT).unwrap();
                Ok(wgpu::PollStatus::WaitSucceeded)
            },
            |timeout| {
                assert!(matches!(recv.try_recv(), Err(mpsc::TryRecvError::Empty)));
                assert!(
                    !timeout.is_zero(),
                    "pending callback needs the remaining budget"
                );
                // The callback cannot finish before we reach the receive phase.
                release.send(()).unwrap();
                recv.recv_timeout(timeout)
            },
        );
        callback.join().unwrap();
        result.unwrap();
    });
}

#[test]
fn completed_callback_is_not_lost_at_the_deadline() {
    let (send, recv) = mpsc::sync_channel(1);
    send.send(Ok(())).unwrap();
    wait_for_map(
        Instant::now(),
        |_| Ok(wgpu::PollStatus::WaitSucceeded),
        |timeout| recv.recv_timeout(timeout),
    )
    .unwrap();
}

#[test]
fn expired_overall_deadline_does_not_start_another_wait() {
    let (_send, recv) = mpsc::sync_channel(1);
    let result = wait_for_map(
        Instant::now(),
        |timeout| {
            assert_eq!(timeout, Duration::ZERO);
            Ok(wgpu::PollStatus::WaitSucceeded)
        },
        |timeout| {
            assert_eq!(timeout, Duration::ZERO);
            recv.recv_timeout(timeout)
        },
    );
    assert!(matches!(
        result,
        Err(OffscreenError::Poll(wgpu::PollError::Timeout))
    ));
}

#[test]
fn callback_map_error_and_disconnection_remain_distinct() {
    let (send, recv) = mpsc::sync_channel(1);
    send.send(Err(wgpu::BufferAsyncError)).unwrap();
    let result = wait_for_map(
        Instant::now() + TEST_TIMEOUT,
        |_| Ok(wgpu::PollStatus::WaitSucceeded),
        |timeout| recv.recv_timeout(timeout),
    );
    assert!(matches!(result, Err(OffscreenError::Map(_))));

    drop(send);
    let result = wait_for_map(
        Instant::now() + TEST_TIMEOUT,
        |_| Ok(wgpu::PollStatus::QueueEmpty),
        |timeout| recv.recv_timeout(timeout),
    );
    assert!(matches!(result, Err(OffscreenError::MapCallback)));
}

#[test]
fn poll_errors_retain_their_cause_without_waiting_for_a_callback() {
    for submission in [None, Some((2, 1))] {
        let result = wait_for_map(
            Instant::now() + TEST_TIMEOUT,
            |_| {
                Err(
                    submission.map_or(wgpu::PollError::Timeout, |(requested, successful)| {
                        wgpu::PollError::WrongSubmissionIndex(requested, successful)
                    }),
                )
            },
            |_| panic!("a failed GPU wait must not start a callback wait"),
        );
        match result {
            Err(OffscreenError::Poll(wgpu::PollError::Timeout)) => assert!(submission.is_none()),
            Err(OffscreenError::Poll(wgpu::PollError::WrongSubmissionIndex(
                requested,
                successful,
            ))) => {
                assert_eq!(Some((requested, successful)), submission);
            }
            other => panic!("expected typed poll error, got {other:?}"),
        }
    }
}
