use super::*;

#[test]
fn required_gpu_callback_can_be_pending_after_another_wait_poll_returns() {
    let context = required_context();
    let device = context.device();
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("synchronized delayed map callback"),
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    context.queue().write_buffer(&buffer, 0, &[0x42; 256]);
    let submission = context.queue().submit([]);
    let (send, recv) = mpsc::sync_channel(1);
    let (entered, entered_recv) = mpsc::sync_channel(1);
    let (release, release_recv) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            entered.send(()).unwrap();
            if release_recv.recv_timeout(Duration::from_secs(5)).is_ok() {
                send.send(result).unwrap();
            }
        });
    std::thread::scope(|scope| {
        let poll_submission = submission.clone();
        let poller = scope.spawn(move || {
            device.poll(wgpu::PollType::Wait {
                submission_index: Some(poll_submission),
                timeout: Some(Duration::from_secs(5)),
            })
        });
        let result = wait_for_map(
            Instant::now() + Duration::from_secs(5),
            |timeout| {
                entered_recv.recv_timeout(timeout).unwrap();
                device.poll(wgpu::PollType::Wait {
                    submission_index: Some(submission),
                    timeout: Some(timeout),
                })
            },
            |timeout| {
                assert!(matches!(recv.try_recv(), Err(mpsc::TryRecvError::Empty)));
                assert!(
                    !timeout.is_zero(),
                    "callback wait must retain its remaining budget"
                );
                release.send(()).unwrap();
                recv.recv_timeout(timeout)
            },
        );
        poller.join().unwrap().unwrap();
        result.unwrap();
    });
    {
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        assert_eq!(&*mapped, &[0x42; 256]);
    }
    buffer.unmap();
}
