mod borders;
mod outer;
use super::{
    composition::{MaterialImage, MaterialTexel},
    mip_gpu::PageWriter,
    residency::ResidentPages,
    residency_tests::{context, settings},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};

#[test]
fn terrain_pages_gpu_sampling_uses_nearest_resident_ancestor_and_filter_safe_lod() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.interior = [4, 4];
    parts.border = 2;
    parts.upload_bytes_per_frame = 1000;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let cache =
        ResidentPages::new(device, &authority, settings, 2, AllocationBudget(100_000)).unwrap();
    let writer = PageWriter::new(device, settings);
    let child = PageKey::new(1, [0, 0]).unwrap();
    for (key, color) in [
        (PageKey::ROOT, [64, 128, 192, 255]),
        (child, [192, 64, 128, 255]),
    ] {
        let mut frame = authority.begin(device).unwrap();
        let pending = cache.reserve(&frame, key).unwrap();
        let base = MaterialImage::new(
            settings.extent(),
            vec![
                MaterialTexel {
                    color,
                    normal_roughness: [128, 128, 255, 180]
                };
                64
            ],
        )
        .unwrap();
        writer
            .encode(device, &mut frame, &cache, pending, &base)
            .unwrap();
        let submission = frame.finish().submit().unwrap();
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .unwrap();
        let bytes = sample(
            device,
            &authority,
            &cache,
            &[child],
            &[[0.25, 0.25, 0.0, 0.0], [0.25, 0.25, 0.5, 0.0]],
        );
        let first = if key == PageKey::ROOT {
            [64, 128, 192, 255]
        } else {
            [192, 64, 128, 255]
        };
        assert_eq!(&bytes[..4], first, "nearest resident at magnification");
        assert_eq!(
            &bytes[4..8],
            [64, 128, 192, 255],
            "coarse footprint uses root, never unsafe child mip"
        );
    }
}

pub(super) mod harness;
use harness::sample;
