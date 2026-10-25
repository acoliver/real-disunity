//! Explicit synthetic timestamp probe. Run serially with `--ignored --nocapture`.
use super::post_effect_tests::{output, paint};
use super::*;
use crate::{Bloom, GaussianLevel, ToneMap, ToneMapOutput};

struct BenchmarkFactory;
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
#[ignore = "explicit 1600x900 GPU performance probe"]
fn gaussian_bloom_stage_timestamps() {
    let (device, queue) =
        crate::gpu_tests::gpu_with_optional_features(wgpu::Features::TIMESTAMP_QUERY);
    let chain = PostChain::new(
        &device,
        FrameConfig::new([1600, 900], wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        BenchmarkFactory,
    )
    .unwrap();
    chain
        .configure_timing(&device, &queue, crate::TimingMode::Required)
        .unwrap();
    // Discard warm-up frames, then report the median of twenty samples per stage.
    let samples: Vec<_> = (0..25)
        .map(|frame| sample(&device, &queue, &chain, frame))
        .skip(5)
        .collect();
    for pass in 0..samples[0].passes.len() {
        let mut durations: Vec<_> = samples
            .iter()
            .map(|frame| frame.passes[pass].duration_ns.expect("valid query pair"))
            .collect();
        durations.sort_by(f64::total_cmp);
        let median = (durations[9] + durations[10]) * 0.5;
        eprintln!(
            "{:?}: median {median:.0} ns (20 samples)",
            samples[0].passes[pass].pass_id
        );
    }
}
