use crate::{
    configuration::{AllocationBudget, AllocationSet, Configuration, ConfigurationError},
    submission::{
        PrepareError, SubmissionAuthority, SubmitError,
        capture::{CaptureError, CaptureWrites},
    },
};
use std::cell::Cell;

fn allocations() -> AllocationSet {
    let mut set = AllocationSet::default();
    set.buffer("capture configuration", 4, 1);
    set
}
fn buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("capture configuration"),
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn assert_clone_busy(device: &wgpu::Device, cloned: &SubmissionAuthority) {
    assert!(matches!(cloned.begin(device), Err(SubmitError::Busy)));
    assert!(matches!(
        cloned.prepare(device, |_, _| Ok::<_, ()>(())),
        Err(PrepareError::Busy)
    ));
    assert!(matches!(cloned.submit_commands([]), Err(SubmitError::Busy)));
    assert!(matches!(cloned.try_configuration(), Err(SubmitError::Busy)));
    assert!(matches!(
        cloned.begin_capture(
            device,
            CaptureWrites::new([], [], []).unwrap(),
            (),
            |_, _, ()| Ok::<_, ()>(())
        ),
        Err(CaptureError::Submission(SubmitError::Busy))
    ));
}

fn assert_excluded(
    device: &wgpu::Device,
    cloned: &SubmissionAuthority,
    configuration: &mut Configuration<wgpu::Buffer>,
    calls: &Cell<u64>,
) {
    assert_clone_busy(device, cloned);
    let generation = configuration.generation();
    let current = configuration.current().clone();
    let resident = configuration.resident_bytes(&device.limits()).unwrap();
    let replacement = configuration.replace(&device.limits(), allocations(), || {
        calls.set(calls.get() + 1);
        Ok::<_, ()>(buffer(device))
    });
    assert!(matches!(
        replacement,
        Err(ConfigurationError::Submission(SubmitError::Busy))
    ));
    assert_eq!(calls.get(), generation);
    assert_eq!(configuration.generation(), generation);
    assert_eq!(configuration.current(), &current);
    assert_eq!(
        configuration.resident_bytes(&device.limits()).unwrap(),
        resident
    );
}

#[test]
fn cloned_authority_and_budgeted_replacement_stay_busy_until_capture_retires() {
    let instance = crate::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    let authority = SubmissionAuthority::new(queue);
    let cloned = authority.clone();
    let mut configuration = Configuration::new(
        &authority,
        buffer(&device),
        allocations(),
        &device.limits(),
        AllocationBudget(8),
    )
    .unwrap();
    let calls = Cell::new(0);
    for complete in [true, false] {
        let generation = configuration.generation();
        let current = configuration.current().clone();
        let continuation = authority
            .begin_capture(
                &device,
                CaptureWrites::new([], [], []).unwrap(),
                (),
                |_, _, ()| Ok::<_, ()>(()),
            )
            .unwrap()
            .submit()
            .unwrap();
        assert_excluded(&device, &cloned, &mut configuration, &calls);
        if complete {
            continuation
                .resume(&device, |_, _| Ok::<_, ()>(()), |_, _, (), ()| Ok(()))
                .unwrap()
                .submit()
                .unwrap()
                .finish(&device, |_, _| Ok::<_, ()>(()))
                .unwrap();
        } else {
            drop(continuation);
            cloned.drain_captures(&device).unwrap();
        }
        configuration
            .replace(&device.limits(), allocations(), || {
                calls.set(calls.get() + 1);
                Ok::<_, ()>(buffer(&device))
            })
            .unwrap();
        assert_eq!(calls.get(), generation + 1);
        assert_eq!(configuration.generation(), generation + 1);
        assert_ne!(configuration.current(), &current);
        assert_eq!(configuration.resident_bytes(&device.limits()).unwrap(), 4);
        cloned
            .prepare(&device, |_, _| Ok::<_, ()>(()))
            .unwrap()
            .cancel();
    }
}
