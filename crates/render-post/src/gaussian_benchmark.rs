//! Explicit synthetic timestamp probe. Run serially with `--ignored --nocapture`.
use super::post_effect_tests::{output, paint};
use super::*;
use crate::{Bloom, GaussianLevel, ToneMap, ToneMapOutput};

struct BenchmarkFactory {
    by_value: bool,
}
impl StageFactory for BenchmarkFactory {
    fn build(
        &self,
        device: &wgpu::Device,
        targets: &FrameTargets,
        resources: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set =
            ToneMapOutput::new(wgpu::TextureFormat::Rgba8UnormSrgb, 1.0, ToneMap::Reinhard)?
                .build(device, targets, resources)?;
        let output = set.stages.pop().unwrap();
        let mut gaussian = crate::gaussian_gpu::GaussianPyramid::new(device, &mut set);
        if self.by_value {
            gaussian.use_by_value();
        }
        let levels = [44, 32, 20, 12, 6, 0, 0, 0].map(|radius| GaussianLevel {
            radius,
            tint: [1.0; 3],
            weight: 0.2,
        });
        gaussian.build(
            targets,
            Bloom {
                threshold: 1.0,
                knee: 0.5,
                intensity: 1.0,
                levels: 5,
            },
            &levels,
        );
        set.stages.push(output);
        Ok(set)
    }
}

fn sample(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chain: &PostChain<BenchmarkFactory>,
    frame: u64,
) -> crate::FrameTiming {
    let target = output(device, [1600, 900]);
    let authority = crate::test_authority(queue);
    let mut encoder = authority.begin(device).unwrap();
    chain.begin_timing(frame).unwrap();
    paint(
        device,
        &mut encoder,
        chain.targets().resolved(),
        "let v=0.25+8.0*step(0.75,fract(p.x/97.0))*step(0.75,fract(p.y/61.0));return vec4<f32>(v,v*0.5,v*0.25,1.0);",
    );
    chain.encode(
        &mut encoder,
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    chain.finish_timing(&mut encoder).unwrap();
    let submission = encoder.finish().submit().unwrap();
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
    chain
        .take_timings()
        .unwrap()
        .pop()
        .expect("required timestamp result")
}

#[test]
#[ignore = "explicit 1600x900 GPU performance comparison"]
fn gaussian_weight_access_timestamp_comparison() {
    let (device, queue) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::TIMESTAMP_QUERY);
    let chains = [false, true].map(|by_value| {
        let chain = PostChain::new(
            &device,
            FrameConfig::new([1600, 900], wgpu::TextureFormat::Rgba16Float, 1),
            Resources::default(),
            BenchmarkFactory { by_value },
        )
        .unwrap();
        chain
            .configure_timing(&device, &queue, crate::TimingMode::Required)
            .unwrap();
        chain
    });
    let mut samples = [Vec::new(), Vec::new()];
    // Alternate designs to reduce clock and temperature ordering bias. Warm up both.
    for frame in 0..25 {
        for index in 0..2 {
            let result = sample(&device, &queue, &chains[index], frame);
            if frame >= 5 {
                samples[index].push(result);
            }
        }
    }
    compare_results(&device, &queue, &chains);
    for (index, design) in ["direct", "by-value"].into_iter().enumerate() {
        for pass in 0..samples[index][0].passes.len() {
            let mut durations: Vec<_> = samples[index]
                .iter()
                .map(|frame| frame.passes[pass].duration_ns.expect("valid query pair"))
                .collect();
            durations.sort_by(f64::total_cmp);
            let median = (durations[9] + durations[10]) * 0.5;
            eprintln!(
                "{design} {:?}: median {median:.0} ns (20 samples)",
                samples[index][0].passes[pass].pass_id
            );
        }
    }
}

fn compare_results(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    chains: &[PostChain<BenchmarkFactory>; 2],
) {
    let images = chains.each_ref().map(|chain| {
        let texture = &chain.configured[0].resources.textures[0];
        crate::Readback::new(
            device,
            FrameConfig::new([texture.width(), texture.height()], texture.format(), 1),
            16_000_000,
        )
        .unwrap()
        .read(device, &crate::test_authority(queue), texture)
        .unwrap()
    });
    for (direct, by_value) in images[0].chunks_exact(2).zip(images[1].chunks_exact(2)) {
        let direct = crate::tone_map_gpu_tests::half(direct);
        let by_value = crate::tone_map_gpu_tests::half(by_value);
        assert!((direct - by_value).abs() <= 0.001 * direct.abs() + 0.000_01);
    }
}
