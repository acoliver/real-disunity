use super::*;
use crate::*;

#[test]
fn raw_counter_pairs_reject_unusable_values_but_allow_resolution_limited_intervals() {
    assert_eq!(ticks_to_ns(0, 0, 2.0), None);
    assert_eq!(ticks_to_ns(13, 10, 2.0), None);
    assert_eq!(ticks_to_ns(10, 10, 2.0), Some(0.0));
    assert_eq!(ticks_to_ns(10, 13, 2.0), Some(6.0));
}

#[test]
fn gpu_overfull_pass_lists_drop_whole_samples_and_release_timing_admission() {
    let (device, queue) = device(wgpu::Features::TIMESTAMP_QUERY);
    let mut timer = PassTimer::new(&device, &queue, TimingMode::Optional).unwrap();
    if timer.support() == TimingSupport::Unsupported {
        return;
    }
    timer.begin_frame(7);
    for _ in 0..MAX_PASSES {
        assert!(timer.stamp(PassId::Geometry(PassSlot::Opaque)).is_some());
    }
    assert!(timer.stamp(PassId::Resolve).is_none());
    let mut encoder = worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
        .begin(&device)
        .unwrap();
    timer.finish_frame(&mut encoder);
    assert_eq!(timer.dropped_frames(), 1);
    assert!(timer.take_ready().unwrap().is_empty());
    timer.begin_frame(8);
    assert!(timer.active.is_some());
    timer.finish_frame(&mut encoder);
    encoder.finish().submit().unwrap();
    device.poll(wgpu::PollType::Poll).unwrap();
}

fn device(features: wgpu::Features) -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: features & adapter.features(),
        ..Default::default()
    }))
    .unwrap()
}
#[test]
fn unavailable_optional_is_unsupported_and_required_fails() {
    let (device, queue) = device(wgpu::Features::empty());
    assert_eq!(
        PassTimer::new(&device, &queue, TimingMode::Optional)
            .unwrap()
            .support(),
        TimingSupport::Unsupported
    );
    assert!(matches!(
        PassTimer::new(&device, &queue, TimingMode::Required),
        Err(TimingError::Unsupported)
    ));
    assert_eq!(
        PassTimer::new(&device, &queue, TimingMode::Disabled)
            .unwrap()
            .support(),
        TimingSupport::Disabled
    );
    assert_eq!(ticks_to_ns(100, 125, 2.5), Some(62.5));
    assert_eq!(ticks_to_ns(u64::MAX - 1, 1, 2.0), None);
    assert_eq!(ticks_to_ns((1_u64 << 36) - 2, 1, 2.0), None);
    assert_eq!(ticks_to_ns(1_u64 << 60, (1_u64 << 60) + 3, 2.0), Some(6.0));
}

struct Factory;
impl StageFactory for Factory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        resources: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set =
            CopyOutput::new(device, targets.config().format).build(device, targets, resources)?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        set.stages.insert(
            0,
            Stage {
                id: crate::StageId::new("compute probe", 0),
                label: "compute probe",
                enabled: true,
                operation: Operation::Compute {
                    pipeline,
                    groups: vec![],
                    dispatch: [1; 3],
                },
            },
        );
        Ok(set)
    }
}

#[test]
fn gpu_timing_covers_executor_order_and_unread_ring_never_blocks_rendering() {
    let (device, queue) = device(wgpu::Features::TIMESTAMP_QUERY);
    let mut chain = PostChain::new(
        &device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 4),
        Resources::default(),
        Factory,
    )
    .unwrap();
    let support = chain
        .configure_timing(&device, &queue, TimingMode::Optional)
        .unwrap();
    eprintln!("timestamp capability branch: {support:?}");
    let available = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);
    assert_eq!(
        support,
        if available {
            TimingSupport::Supported
        } else {
            TimingSupport::Unsupported
        }
    );
    let (passes, views) = fixture(&device, &chain);
    let output = color_texture(
        &device,
        "timed output",
        [8, 8],
        wgpu::TextureFormat::Rgba8Unorm,
    );
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    render_frames(&device, &queue, &mut chain, &passes, &views, &output_view);
    let frames = chain.take_timings().unwrap();
    assert_results(&device, &queue, &chain, &passes, &output_view, frames);
    let bytes = Readback::new(
        &device,
        FrameConfig::new([8, 8], wgpu::TextureFormat::Rgba8Unorm, 1),
        4096,
    )
    .unwrap()
    .read(&device, &crate::test_authority(&queue), &output)
    .unwrap();
    assert!(bytes.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]));
}
fn fixture(
    device: &wgpu::Device,
    chain: &PostChain<Factory>,
) -> (Vec<PassDeclaration>, Vec<AttachmentView>) {
    let color = ResourceRef::new("color");
    let depth = ResourceRef::new("depth");
    let mut views = vec![
        AttachmentView::new(color, chain.targets().scene().texture()).unwrap(),
        AttachmentView::new(depth, chain.targets().depth().texture()).unwrap(),
    ];
    let mut passes = Vec::new();
    for cascade in 0..4 {
        let resource = ResourceRef {
            layer: 0,
            version: cascade,
            ..ResourceRef::new("shadow")
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        views.push(AttachmentView::new(resource, &texture).unwrap());
        let mut pass = PassDeclaration::new(PassSlot::ShadowCasters(cascade));
        pass.depth = Some(AttachmentDeclaration {
            resource,
            format: wgpu::TextureFormat::Depth32Float,
            size: [8, 8],
            samples: 1,
            load: AttachmentLoad::ClearDepth(1.0),
            store: wgpu::StoreOp::Store,
        });
        passes.push(pass);
    }
    let mut opaque = PassDeclaration::new(PassSlot::Opaque);
    opaque.color = Some(AttachmentDeclaration {
        resource: color,
        format: wgpu::TextureFormat::Rgba8Unorm,
        size: [8, 8],
        samples: 4,
        load: AttachmentLoad::ClearColor(wgpu::Color::BLACK),
        store: wgpu::StoreOp::Store,
    });
    opaque.depth = Some(AttachmentDeclaration {
        resource: depth,
        format: wgpu::TextureFormat::Depth32Float,
        size: [8, 8],
        samples: 4,
        load: AttachmentLoad::ClearDepth(1.0),
        store: wgpu::StoreOp::Store,
    });
    passes.push(opaque);
    (passes, views)
}

fn render_frames(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &mut PostChain<Factory>,
    passes: &[PassDeclaration],
    views: &[AttachmentView],
    output_view: &wgpu::TextureView,
) {
    let mut submission = None;
    for frame_id in 10..18 {
        let mut encoder =
            worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
                .begin(device)
                .unwrap();
        chain.begin_timing(frame_id).unwrap();
        chain
            .encode_geometry(
                device,
                &mut encoder,
                &FramePlan {
                    passes,
                    views,
                    requested: &[],
                },
                &mut [],
            )
            .unwrap();
        chain.encode_resolve(&mut encoder);
        chain.encode(&mut encoder, output_view);
        chain.finish_timing(&mut encoder).unwrap();
        submission = Some(encoder.finish().submit().unwrap());
        device.poll(wgpu::PollType::Poll).unwrap();
    }
    device
        .poll(wgpu::PollType::Wait {
            submission_index: submission,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}

fn assert_results(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<Factory>,
    passes: &[PassDeclaration],
    output_view: &wgpu::TextureView,
    frames: Vec<FrameTiming>,
) {
    if device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        assert_eq!(chain.dropped_timing_frames(), 5);
        assert_eq!(
            frames.iter().map(|f| f.frame_id).collect::<Vec<_>>(),
            [10, 11, 12]
        );
        let expected: Vec<_> = passes
            .iter()
            .map(|pass| PassId::Geometry(pass.slot))
            .chain([
                PassId::Resolve,
                PassId::Stage(StageId::new("compute probe", 0)),
                PassId::Stage(StageId::new("post copy output", 0)),
            ])
            .collect();
        let unavailable = frames
            .iter()
            .flat_map(|frame| &frame.passes)
            .filter(|pass| pass.duration_ns.is_none())
            .count();
        assert_eq!(
            chain.unavailable_timing_passes(),
            u64::try_from(unavailable).unwrap()
        );
        for frame in frames {
            assert_eq!(
                frame.passes.iter().map(|p| p.pass_id).collect::<Vec<_>>(),
                expected
            );
            assert!(
                frame
                    .passes
                    .iter()
                    .filter_map(|p| p.duration_ns)
                    .all(|duration| duration.is_finite() && duration >= 0.0)
            );
        }
        assert!(chain.take_timings().unwrap().is_empty());
        let mut encoder =
            worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone())
                .begin(device)
                .unwrap();
        chain.begin_timing(99).unwrap();
        chain.encode(&mut encoder, output_view);
        chain.finish_timing(&mut encoder).unwrap();
        let submitted = encoder.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        assert_eq!(chain.take_timings().unwrap()[0].frame_id, 99);
    } else {
        assert!(frames.is_empty());
    }
}

#[test]
fn gpu_synthetic_counter_wraps_are_unavailable_counted_and_release_slots() {
    let (device, queue) = device(wgpu::Features::TIMESTAMP_QUERY);
    let mut timer = PassTimer::new(&device, &queue, TimingMode::Optional).unwrap();
    eprintln!("wrap timing capability branch: {:?}", timer.support());
    if timer.support() == TimingSupport::Unsupported {
        return;
    }
    for (frame, begin, end) in [
        (1, 0, 0),
        (2, (1_u64 << 36) - 2, 1),
        (3, u64::MAX - 1, 1),
        (4, 10, 13),
        (5, 20, 23),
    ] {
        timer.begin_frame(frame);
        timer.stamp(PassId::Resolve).unwrap();
        assert_eq!(timer.active, Some(0), "completed slots must be reused");
        let slot = &mut timer.slots[timer.active.take().unwrap()];
        let bytes: Vec<_> = [begin, end]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
        queue.write_buffer(&slot.readback, 0, &bytes);
        let submitted = queue.submit([]);
        let (send, receive) = mpsc::sync_channel(1);
        slot.readback
            .slice(..16)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        slot.pending = Some(receive);
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        let ready = timer.take_ready().unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].frame_id, frame);
        assert_eq!(ready[0].passes[0].pass_id, PassId::Resolve);
        if frame < 4 {
            assert_eq!(ready[0].passes[0].duration_ns, None);
            assert_eq!(timer.unavailable_passes(), frame);
        } else {
            assert_eq!(ready[0].passes[0].duration_ns, Some(3.0 * timer.period_ns));
            assert_eq!(timer.unavailable_passes(), 3);
        }
        assert_eq!(timer.dropped_frames(), 0);
        assert!(timer.slots.iter().all(|slot| slot.pending.is_none()));
    }
}
