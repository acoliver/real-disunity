use crate::configuration::*;
use crate::submission::SubmissionAuthority;
use std::cell::Cell;

#[test]
fn volume_storage_halves_the_depth_of_each_level_and_counts_levels_by_the_longest_side() {
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let limits = wgpu::Limits::default();
    // 2x2x8 texels of four bytes: levels of 2x2x8, 1x1x4, 1x1x2 and 1x1x1 hold 32, 4, 2 and 1
    // texels. The longest side is the depth, so four levels are a whole chain.
    let mut chain = AllocationSet::default();
    chain.volume("volume", [2, 2, 8], format, 4);
    assert_eq!(chain.validate(&limits).unwrap(), 39 * 4);
    let mut too_long = AllocationSet::default();
    too_long.volume("volume", [2, 2, 8], format, 5);
    assert!(too_long.validate(&limits).is_err());
    // A volume deeper than the 2D array limit is only held to the limit of 3D extents.
    let deep = limits.max_texture_array_layers + 1;
    let mut tall = AllocationSet::default();
    tall.volume("volume", [1, 1, deep], format, 1);
    assert_eq!(tall.validate(&limits).unwrap(), u64::from(deep) * 4);
    assert!(tall.validate_array_layers::<()>(&limits).is_ok());
    let mut beyond = AllocationSet::default();
    beyond.volume(
        "volume",
        [1, 1, limits.max_texture_dimension_3d + 1],
        format,
        1,
    );
    assert!(beyond.validate(&limits).is_err());
}

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
    let gpu = crate::test_support::TestGpu::shared(&crate::device::DeviceRequirements::default())
        .unwrap();
    (gpu.device, SubmissionAuthority::new(gpu.queue))
}
#[test]
fn derived_budget_tracks_all_live_generations_and_shared_reservations() {
    let (device, authority) = fresh_context();
    let inventory = |size| {
        let mut set = bytes(size);
        set.shared_cpu("host capture pool", 256);
        set
    };
    let policy = AllocationBudgetPolicy::Derived {
        headroom_bytes: 8,
        granularity_bytes: 16,
        cap_bytes: Some(512),
    };
    let mut config =
        Configuration::new(&authority, 1, inventory(16), &device.limits(), policy).unwrap();
    assert_eq!(config.budget(), AllocationBudget(288));
    let flight = authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    config
        .replace(&device.limits(), inventory(24), || Ok::<_, ()>(2))
        .unwrap();
    assert_eq!(config.budget(), AllocationBudget(304));
    config
        .replace(&device.limits(), inventory(28), || Ok::<_, ()>(3))
        .unwrap();
    assert_eq!(config.budget(), AllocationBudget(336));
    assert_eq!(config.resident_bytes(&device.limits()).unwrap(), 324);
    assert!(matches!(
        config.replace(&device.limits(), inventory(256), || Ok::<_, ()>(4)),
        Err(ConfigurationError::Budget {
            required: 580,
            budget: 512
        })
    ));
    assert_eq!(config.budget(), AllocationBudget(336));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(flight),
            timeout: None,
        })
        .unwrap();
    config.collect();
    config
        .replace(&device.limits(), inventory(16), || Ok::<_, ()>(4))
        .unwrap();
    assert_eq!(config.budget(), AllocationBudget(320));
}

fn fresh_context() -> (wgpu::Device, SubmissionAuthority) {
    let (device, queue) = crate::test_support::fresh_device();
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
fn external_storage_counts_its_bytes_without_the_buffer_rules() {
    let limits = wgpu::Limits::default();
    let mut set = AllocationSet::default();
    set.external("odd texture", 3);
    set.external("past the buffer limit", limits.max_buffer_size + 1);
    assert_eq!(set.validate(&limits).unwrap(), limits.max_buffer_size + 4);
    let mut empty = AllocationSet::default();
    empty.external("empty", 0);
    assert_eq!(empty.validate(&limits), Err("empty external storage"));
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
    let (device, authority) = fresh_context();
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
fn retained_arrays_buffers_and_cpu_reservations_are_counted_once_at_peak() {
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
    retained.shared_cpu("recipe pool", 256 * 1024 * 1024);
    let mut initial = retained.clone();
    initial.buffer("targets", 16, 1);
    let mut config = Configuration::new(
        &authority,
        1,
        initial,
        &device.limits(),
        AllocationBudget(256 * 1024 * 1024 + 2096),
    )
    .unwrap();
    let mut replacement = retained;
    replacement.buffer("targets", 16, 1);
    config
        .replace(&device.limits(), replacement, || Ok::<_, ()>(2))
        .unwrap();
    assert_eq!(
        config.resident_bytes(&device.limits()).unwrap(),
        256 * 1024 * 1024 + 2048 + 16 + 16
    );
}

#[test]
fn a_reservation_reports_the_descriptor_bytes_it_holds() {
    let (device, authority) = context();
    let mut config = Configuration::new(
        &authority,
        1,
        bytes(16),
        &device.limits(),
        AllocationBudget(1024),
    )
    .unwrap();
    let mut arena = AllocationSet::default();
    arena.shared_buffer("arena", 512, 1);
    let reservation = config.reserve::<()>(&device.limits(), arena).unwrap();
    assert_eq!(reservation.bytes(&device.limits()), Ok(512));
    let mut large = AllocationSet::default();
    large.buffer("too large", 1024, 1);
    assert!(matches!(
        config.reserve::<()>(&device.limits(), large),
        Err(ConfigurationError::Budget { .. })
    ));
}

#[test]
fn a_counted_configuration_counts_gpu_generations_until_they_retire() {
    use crate::census::ResourceClass;
    let (device, authority) = fresh_context();
    let census = authority.census().clone();
    let mut first = bytes(16);
    first.cpu("host copy", 1000);
    let mut config = Configuration::new(
        &authority,
        1_u32,
        first,
        &device.limits(),
        AllocationBudget(4096),
    )
    .unwrap()
    .counted(ResourceClass::RenderTarget, &device.limits());
    // The CPU entry counts toward admission but not toward resident GPU memory.
    assert_eq!(census.snapshot().get(ResourceClass::RenderTarget), 16);
    let flight = authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    config
        .replace(&device.limits(), bytes(24), || Ok::<_, ()>(2))
        .unwrap();
    // The old generation stays counted until the queue passes its last submission.
    assert_eq!(config.retired_count(), 1);
    assert_eq!(census.snapshot().get(ResourceClass::RenderTarget), 40);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(flight),
            timeout: None,
        })
        .unwrap();
    config.collect();
    assert_eq!(census.snapshot().get(ResourceClass::RenderTarget), 24);
    drop(config);
    assert_eq!(census.snapshot().get(ResourceClass::RenderTarget), 0);
}

#[test]
fn an_uncounted_configuration_leaves_the_census_alone() {
    let (device, authority) = fresh_context();
    let config = Configuration::new(
        &authority,
        (),
        bytes(16),
        &device.limits(),
        AllocationBudget(64),
    )
    .unwrap();
    assert_eq!(authority.census().snapshot().total(), 0);
    drop(config);
}

/// A retired generation's frame is complete on the GPU, but a held poller has not run its
/// completion bookkeeping (hub issue 4d2b47c55165). Its bytes still count, so the next
/// replacement fails the budget; collecting settled waits for the bookkeeping and frees them.
#[test]
fn collecting_settled_frees_a_generation_whose_bookkeeping_a_held_poller_holds() {
    let (device, authority) = fresh_context();
    let mut config = Configuration::new(
        &authority,
        1_u32,
        bytes(16),
        &device.limits(),
        AllocationBudget(40),
    )
    .unwrap();
    let _flight = authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    config
        .replace(&device.limits(), bytes(16), || Ok::<_, ()>(2))
        .unwrap();
    let held = crate::test_support::HeldPoller::start(&device);
    assert!(
        matches!(
            config.replace(&device.limits(), bytes(16), || Ok::<_, ()>(3)),
            Err(ConfigurationError::Budget { .. })
        ),
        "the retired generation still counts before its completion is recorded"
    );
    let releaser = held.release_after(std::time::Duration::from_millis(200));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    config.collect_settled(&device, deadline).unwrap();
    assert_eq!(config.retired_count(), 0);
    config
        .replace(&device.limits(), bytes(16), || Ok::<_, ()>(3))
        .expect("the freed generation no longer counts");
    releaser.join().unwrap();
    held.join();
}

#[test]
fn a_kept_backing_counts_once_and_a_replaced_backing_counts_beside_it_until_it_retires() {
    let (device, authority) = fresh_context();
    let limits = device.limits();
    let backing = |id: u64| {
        let mut set = AllocationSet::default();
        set.backing_texture(
            "history",
            id,
            [8, 8, 1],
            wgpu::TextureFormat::Rgba8Unorm,
            [1, 1],
        );
        set.buffer("targets", 16, 1);
        set
    };
    let mut config =
        Configuration::new(&authority, 1, backing(0), &limits, AllocationBudget(800)).unwrap();
    let flight = authority
        .prepare(&device, |_, _| Ok::<_, ()>(()))
        .unwrap()
        .submit()
        .unwrap();
    // The kept backing is one storage in both generations; each generation has its own buffer.
    config
        .replace(&limits, backing(0), || Ok::<_, ()>(2))
        .unwrap();
    assert_eq!(config.resident_bytes(&limits).unwrap(), 256 + 16 + 16);
    // A different backing under the same name, at the same size, is further storage.
    config
        .replace(&limits, backing(1), || Ok::<_, ()>(3))
        .unwrap();
    assert_eq!(config.resident_bytes(&limits).unwrap(), 256 + 256 + 3 * 16);
    // A third distinct backing does not fit beside the two that are still live.
    assert!(matches!(
        config.replace(&limits, backing(2), || -> Result<i32, ()> {
            panic!("must not allocate")
        }),
        Err(ConfigurationError::Budget { .. })
    ));
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(flight),
            timeout: None,
        })
        .unwrap();
    config.collect();
    assert_eq!(config.resident_bytes(&limits).unwrap(), 256 + 16);
}
