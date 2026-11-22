use super::*;
use crate::GpuLaunch;
use worldrunner_world_simulation::graphics::WorldSampler;

fn id(bits: u64) -> RenderTextureId {
    RenderTextureId::from_bits(bits)
}

#[test]
fn a_texture_missing_from_the_snapshot_is_destroyed() {
    let directory = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(directory.path(), &GpuLaunch::default()).unwrap();

    let offers = gpu.ui_textures([id(1)], |_| None);

    assert!(matches!(
        offers.as_slice(),
        [(_, UiOffer::Unavailable("destroyed"))]
    ));
}

#[test]
fn a_texture_the_world_holds_and_no_frame_made_is_unwritten_with_its_sampler() {
    let directory = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(directory.path(), &GpuLaunch::default()).unwrap();
    let clamped = WorldSampler {
        wrap_u: 1,
        wrap_v: 1,
        filter: 0,
        ..WorldSampler::default()
    };

    let offers = gpu.ui_textures([id(1)], |_| Some(clamped));

    let [(_, UiOffer::Unwritten(target))] = offers.as_slice() else {
        panic!("{offers:?}");
    };
    assert!(target.texture().is_none());
    assert_eq!(target.sampler().wrap_u, TextureWrap::Clamp);
    assert_eq!(target.sampler().filter, TextureFilter::Point);
}
