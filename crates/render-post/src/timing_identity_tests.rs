use super::post_effect_tests::{adaptive, chain, output, paint};
use super::*;
use crate::*;

fn submit(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<ToneMapOutput>,
    frame: u64,
) {
    let target = output(device, chain.targets().config().size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.begin_timing(frame);
    paint(
        device,
        &mut encoder,
        chain.targets().resolved(),
        "return vec4(0.75, 0.0, 0.0, 1.0);",
    );
    chain
        .factory()
        .prepare_effects(&mut encoder, 0, 0.031, true)
        .unwrap();
    chain.encode(
        &mut encoder,
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    chain.finish_timing(&mut encoder);
    let submitted = queue.submit([encoder.finish()]);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submitted),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
}

#[test]
fn unread_post_ids_survive_resize_and_reconfiguration() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
        ..Default::default()
    }))
    .unwrap();
    let settings = HdrPostSettings {
        bloom: Some(Bloom {
            threshold: 1.0,
            knee: 0.5,
            intensity: 0.25,
            levels: 3,
        }),
        ..Default::default()
    };
    let mut chain = chain(&device, settings, [4, 4]);
    let support = chain
        .configure_timing(&device, &queue, TimingMode::Optional)
        .unwrap();
    eprintln!("resize timing capability branch: {support:?}");
    submit(&device, &queue, &chain, 1);
    chain.resize(&device, [1, 1]).unwrap();
    submit(&device, &queue, &chain, 2);
    chain.resize(&device, [4, 4]).unwrap();
    chain
        .reconfigure(
            &device,
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)
                .unwrap()
                .with_effects(
                    &device,
                    HdrPostSettings {
                        adaptive: Some(adaptive()),
                        fxaa: Some(1.0),
                        ..settings
                    },
                )
                .unwrap(),
        )
        .unwrap();
    submit(&device, &queue, &chain, 3);
    let frames = chain.take_timings().unwrap();
    if support == TimingSupport::Unsupported {
        assert!(frames.is_empty());
        return;
    }
    assert_eq!(
        frames.iter().map(|f| f.frame_id).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_identities(&frames);
}

fn assert_identities(frames: &[FrameTiming]) {
    let ids: Vec<Vec<_>> = frames
        .iter()
        .map(|frame| frame.passes.iter().map(|p| p.pass_id).collect())
        .collect();
    let id = |name, level| PassId::Stage(StageId::new(name, level));
    assert_eq!(
        ids[0],
        [
            id("prefilter", 0),
            id("downsample", 1),
            id("downsample", 2),
            id("upsample", 1),
            id("upsample", 0),
            id("tone", 0)
        ]
    );
    assert_eq!(ids[1], [id("prefilter", 0), id("tone", 0)]);
    assert_eq!(
        ids[2],
        [
            id("histogram", 0),
            id("reduce", 0),
            id("prefilter", 0),
            id("downsample", 1),
            id("downsample", 2),
            id("upsample", 1),
            id("upsample", 0),
            id("tone", 0),
            id("fxaa", 0)
        ]
    );
    assert!(ids[0].iter().all(|id| ids[2].contains(id)));
    assert!(ids[1].iter().all(|id| ids[0].contains(id)));
}
