use crate::configuration::*;
use crate::submission::SubmissionAuthority;
use std::cell::Cell;

#[test]
fn depth_stencil_storage_accounts_for_backend_promotion() {
    for (format, bytes) in [
        (wgpu::TextureFormat::Stencil8, 8),
        (wgpu::TextureFormat::Depth24Plus, 4),
        (wgpu::TextureFormat::Depth24PlusStencil8, 8),
        (wgpu::TextureFormat::Depth32FloatStencil8, 8),
        (wgpu::TextureFormat::Depth16Unorm, 2),
        (wgpu::TextureFormat::Depth32Float, 4),
    ] {
        let mut set = AllocationSet::default();
        set.texture("depth stencil", [8, 4, 2], format, [3, 1], 3);
        // Mip texels: 32 + 8 + 2, two array layers, three allocations.
        assert_eq!(
            set.validate(&wgpu::Limits::default()).unwrap(),
            42 * 2 * 3 * bytes
        );
        let mut msaa = AllocationSet::default();
        msaa.texture("sampled depth stencil", [8, 4, 2], format, [1, 4], 3);
        assert_eq!(
            msaa.validate(&wgpu::Limits::default()).unwrap(),
            32 * 2 * 4 * 3 * bytes
        );
    }
    let mut stencil = AllocationSet::default();
    stencil.texture(
        "stencil",
        [1024, 1024, 1],
        wgpu::TextureFormat::Stencil8,
        [1, 1],
        1,
    );
    assert!(matches!(
        Configuration::new(
            &context().1,
            (),
            stencil,
            &wgpu::Limits::default(),
            AllocationBudget(1024 * 1024)
        ),
        Err(ConfigurationError::Budget {
            required: 8_388_608,
            budget: 1_048_576
        })
    ));
}

fn context() -> (wgpu::Device, SubmissionAuthority) {
    let instance = crate::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
    (device, SubmissionAuthority::new(queue))
}
fn bytes(size: u64) -> AllocationSet {
    let mut set = AllocationSet::default();
    set.buffer("target", size, 1);
    set
}
#[test]
fn padded_block_mip_and_slot_accounting() {
    let mut set = AllocationSet::default();
    set.texture(
        "color",
        [5, 3, 2],
        wgpu::TextureFormat::Rgba16Float,
        [1, 1],
        3,
    );
    set.readback("capture", [5, 3], 8, 3);
    set.texture(
        "blocks",
        [8, 8, 1],
        wgpu::TextureFormat::Bc1RgbaUnorm,
        [3, 1],
        1,
    );
    set.cpu("accumulation", 5 * 3 * 16);
    // Color: 5*3*2*8*3. Readback: 256*3*3. BC: (4+1+1)*8.
    assert_eq!(
        set.validate(&wgpu::Limits::default()).unwrap(),
        720 + 2304 + 48 + 240
    );
}
#[test]
fn rejection_and_allocator_failure_leave_generation_and_output_unchanged() {
    let (device, authority) = context();
    let mut config = Configuration::new(
        &authority,
        7_u32,
        bytes(16),
        &device.limits(),
        AllocationBudget(40),
    )
    .unwrap();
    let called = Cell::new(false);
    let rejected = config.replace(&device.limits(), bytes(28), || {
        called.set(true);
        Ok::<_, &'static str>(9)
    });
    assert!(matches!(
        rejected,
        Err(ConfigurationError::Budget {
            required: 44,
            budget: 40
        })
    ));
    assert!(!called.get());
    assert_eq!((*config.current(), config.generation()), (7, 0));
    let failed = config.replace(&device.limits(), bytes(20), || {
        Err::<u32, _>("injected allocation failure")
    });
    assert!(matches!(
        failed,
        Err(ConfigurationError::Allocation(
            "injected allocation failure"
        ))
    ));
    assert_eq!((*config.current(), config.generation()), (7, 0));
    assert!(authority.is_idle());
}
#[test]
fn replacement_peak_and_retirement_follow_queue_completion() {
    let (device, authority) = context();
    let mut config = Configuration::new(
        &authority,
        1_u32,
        bytes(16),
        &device.limits(),
        AllocationBudget(64),
    )
    .unwrap();
    let flight = authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    config
        .replace(&device.limits(), bytes(24), || Ok::<_, ()>(2))
        .unwrap();
    assert_eq!(config.retired_count(), 1);
    assert_eq!(config.resident_bytes(&device.limits()).unwrap(), 40);
    assert!(matches!(
        config.replace(&device.limits(), bytes(28), || Ok::<_, ()>(3)),
        Err(ConfigurationError::Budget {
            required: 68,
            budget: 64
        })
    ));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(flight),
            timeout: None,
        })
        .unwrap();
    config.collect();
    assert_eq!(config.retired_count(), 0);
    assert_eq!(config.resident_bytes(&device.limits()).unwrap(), 24);
}
#[test]
fn prepared_work_excludes_replacement_and_limits_precede_allocator() {
    let (device, authority) = context();
    let mut config = Configuration::new(
        &authority,
        1,
        bytes(16),
        &device.limits(),
        AllocationBudget(128),
    )
    .unwrap();
    let prepared = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
    assert!(matches!(
        config.replace(&device.limits(), bytes(16), || Ok::<_, ()>(2)),
        Err(ConfigurationError::Submission(_))
    ));
    prepared.cancel();
    let mut invalid = AllocationSet::default();
    invalid.readback("oversize", [u32::MAX, u32::MAX], 16, 1);
    assert!(matches!(
        config.replace(&device.limits(), invalid, || -> Result<i32, ()> {
            panic!("must not allocate")
        }),
        Err(ConfigurationError::Invalid(_))
    ));
}

#[test]
fn retained_arrays_and_buffers_are_counted_once_at_peak() {
    let (device, authority) = context();
    let mut retained = AllocationSet::default();
    retained.shared_texture(
        "resident arrays",
        [8, 8, 4],
        wgpu::TextureFormat::Rgba8Unorm,
        [1, 1],
        2,
    );
    retained.shared_buffer("history", 16, 1);
    let mut initial = retained.clone();
    initial.buffer("targets", 16, 1);
    let mut config = Configuration::new(
        &authority,
        1,
        initial,
        &device.limits(),
        AllocationBudget(2096),
    )
    .unwrap();
    let mut replacement = retained;
    replacement.buffer("targets", 16, 1);
    config
        .replace(&device.limits(), replacement, || Ok::<_, ()>(2))
        .unwrap();
    assert_eq!(
        config.resident_bytes(&device.limits()).unwrap(),
        2048 + 16 + 16
    );
}
