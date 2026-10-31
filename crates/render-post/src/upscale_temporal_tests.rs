//! Configuration rows of `UpscaleMode::Temporal`: every invalid combination is a typed error
//! before any allocation, and equal extents are the native TAA path.
use crate::{
    FrameConfig, HdrPostSettings, PostError, PostResolve, ProjectionJitter, SpatialUpscale,
    StageFactory, UpscaleMode, UpscaleSettings, allocation::post_allocations,
};

const OUTPUT: [u32; 2] = [64, 48];
const REDUCED: [u32; 2] = [43, 32];

fn settings(mode: UpscaleMode, resolve: PostResolve) -> HdrPostSettings {
    HdrPostSettings {
        resolve,
        upscale: Some(UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    }
}

fn temporal(resolve: PostResolve) -> HdrPostSettings {
    settings(UpscaleMode::Temporal, resolve)
}

fn config(render: [u32; 2], samples: u32) -> FrameConfig {
    FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, samples).with_render_extent(render)
}

fn unsupported(message: &'static str) -> Result<(), PostError> {
    Err(PostError::UnsupportedProfile(message))
}

#[test]
fn temporal_without_taa_is_rejected_at_reduced_and_equal_extents() {
    for resolve in [PostResolve::None, PostResolve::fxaa(1.0)] {
        for render in [REDUCED, OUTPUT] {
            assert_eq!(
                temporal(resolve).validate_frame(config(render, 1)),
                unsupported("temporal upscaling requires a TAA resolve"),
                "{resolve:?} at {render:?}"
            );
        }
    }
}

#[test]
fn temporal_with_msaa_above_one_sample_is_rejected_when_reduced() {
    for samples in [2, 4, 8] {
        assert_eq!(
            temporal(PostResolve::taa(0.9)).validate_frame(config(REDUCED, samples)),
            unsupported("temporal upscaling requires one sample per pixel"),
            "{samples} samples"
        );
    }
}

#[test]
fn temporal_below_half_the_output_on_either_axis_is_rejected() {
    for render in [[31, 48], [64, 23], [31, 23]] {
        assert_eq!(
            temporal(PostResolve::taa(0.9)).validate_frame(config(render, 1)),
            unsupported("temporal upscaling requires a render extent of at least half the output"),
            "{render:?}"
        );
    }
}

#[test]
fn temporal_accepts_exactly_half_the_output_up_to_the_jitter_check() {
    let post = temporal(PostResolve::taa(0.9));
    let jitter = ProjectionJitter::for_extents([32, 24], OUTPUT).unwrap();
    assert_eq!(
        post.validate_frame_with_jitter(config([32, 24], 1), Some(jitter)),
        Ok(())
    );
}

#[test]
fn explicit_jitter_shorter_than_the_derived_sequence_is_rejected() {
    let post = temporal(PostResolve::taa(0.9));
    let needed = ProjectionJitter::for_extents(REDUCED, OUTPUT).unwrap();
    assert!(needed.length() > ProjectionJitter::DEFAULT_LENGTH);
    for length in [1, ProjectionJitter::DEFAULT_LENGTH, needed.length() - 1] {
        let short = ProjectionJitter::new(length).unwrap();
        assert_eq!(
            post.validate_frame_with_jitter(config(REDUCED, 1), Some(short)),
            unsupported(
                "temporal upscaling requires a jitter sequence at least as long as the derived one"
            ),
            "length {length}"
        );
    }
    for length in [needed.length(), needed.length() + 1] {
        let enough = ProjectionJitter::new(length).unwrap();
        assert_eq!(
            post.validate_frame_with_jitter(config(REDUCED, 1), Some(enough)),
            Ok(()),
            "length {length}"
        );
    }
    // No explicit choice means the derived sequence, which is long enough by definition.
    assert_eq!(
        post.validate_frame_with_jitter(config(REDUCED, 1), None),
        Ok(())
    );
}

#[test]
fn equal_extents_with_temporal_and_taa_are_the_native_taa_path() {
    let taa = PostResolve::taa(0.9);
    let native = HdrPostSettings {
        resolve: taa,
        ..HdrPostSettings::default()
    };
    let same = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(temporal(taa).validate_frame(same), Ok(()));
    let msaa = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 4);
    assert_eq!(temporal(taa).validate_frame(msaa), Ok(()));
    let limits = wgpu::Limits::default();
    let bytes = |settings| post_allocations(same, settings).validate(&limits).unwrap();
    assert_eq!(bytes(temporal(taa)), bytes(native));
}

#[test]
fn existing_upscale_modes_keep_their_reduced_taa_configurations() {
    for mode in [UpscaleMode::Bilinear, UpscaleMode::Easu] {
        let post = settings(mode, PostResolve::taa(0.9));
        assert_eq!(post.validate_frame(config(REDUCED, 1)), Ok(()), "{mode:?}");
        assert_eq!(post.validate_frame(config(REDUCED, 4)), Ok(()), "{mode:?}");
    }
}

#[test]
fn temporal_is_not_a_standalone_spatial_upscale() {
    let (device, _queue) = crate::gpu_tests::gpu();
    let factory = SpatialUpscale {
        settings: UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        },
        output_format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let ldr =
        FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba8Unorm, 1).with_render_extent(REDUCED);
    assert_eq!(
        factory.validate_config(&device, ldr),
        unsupported("temporal upscaling requires the HDR scene profile")
    );
}

#[test]
fn reduced_temporal_derives_its_jitter_from_both_extents() {
    let derived = ProjectionJitter::for_extents(REDUCED, OUTPUT).unwrap();
    let reduced = temporal(PostResolve::taa(0.9));
    assert_eq!(
        reduced.effective_jitter(config(REDUCED, 1), None),
        Ok(Some(derived))
    );
    assert!(derived.length() > ProjectionJitter::default().length());
}

#[test]
fn an_explicit_jitter_and_the_other_modes_keep_the_resolve_default() {
    let long = ProjectionJitter::new(64).unwrap();
    let reduced = temporal(PostResolve::taa(0.9));
    assert_eq!(
        reduced.effective_jitter(config(REDUCED, 1), Some(long)),
        Ok(Some(long))
    );
    // Equal extents are the native TAA: the 8 phase default.
    assert_eq!(
        reduced.effective_jitter(config(OUTPUT, 1), None),
        Ok(Some(ProjectionJitter::default()))
    );
    let plain = HdrPostSettings::default();
    assert_eq!(plain.effective_jitter(config(OUTPUT, 1), None), Ok(None));
}
