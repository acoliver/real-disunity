use crate::{FrameProfile, PresentationBudget, PresentationBudgetPolicy, WindowSizeUnit};
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};

#[test]
fn window_units_preserve_requested_backing_size() {
    for (scale, multiplier) in [(1.0, 1), (2.0, 2)] {
        let physical = WindowSizeUnit::Physical
            .creation_size([800, 600])
            .to_physical::<u32>(scale);
        let logical = WindowSizeUnit::Logical
            .creation_size([800, 600])
            .to_physical::<u32>(scale);
        assert_eq!([physical.width, physical.height], [800, 600]);
        assert_eq!(
            [logical.width, logical.height],
            [800 * multiplier, 600 * multiplier]
        );
    }
    assert_eq!(WindowSizeUnit::default(), WindowSizeUnit::Logical);
    assert_eq!(
        PresentationBudgetPolicy::default(),
        PresentationBudgetPolicy::Fixed(PresentationBudget::default())
    );
}

#[test]
fn descriptor_plans_cover_profiles_scales_and_recipe_reservation() {
    let limits = wgpu::Limits::default();
    for profile in profiles() {
        let options = crate::ViewerOptions {
            frame_profile: profile,
            ..crate::tests::options()
        };
        for capture in [
            crate::PresentationCapture::Ordinary,
            crate::PresentationCapture::Retained,
        ] {
            let mut previous = 0;
            for size in [[800, 600], [1600, 1200]] {
                let set = crate::plan_presentation(
                    size,
                    &options,
                    None,
                    &limits,
                    wgpu::Features::empty(),
                    crate::PresentationPlanning {
                        capture,
                        retained_effect_history: false,
                    },
                )
                .unwrap();
                let required = set.validate(&limits).unwrap();
                assert!(required > previous);
                previous = required;
                assert_policy_admission(&set, &limits, required);
            }
        }
    }
}

fn assert_policy_admission(
    set: &worldrunner_render_gpu::configuration::AllocationSet,
    limits: &wgpu::Limits,
    required: u64,
) {
    let policy = PresentationBudgetPolicy::Derived {
        headroom_bytes: 37,
        granularity_bytes: 256,
        cap_bytes: None,
    };
    let effective = policy
        .allocation_policy()
        .resolve::<std::convert::Infallible>(required)
        .unwrap();
    assert!(effective.0 >= required + 37);
    assert_eq!(effective.0 % 256, 0);
    assert!(matches!(
        set.validate_budget::<std::convert::Infallible>(limits, AllocationBudget(required - 1)),
        Err(ConfigurationError::Budget { .. })
    ));
    let capped = PresentationBudgetPolicy::Derived {
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: Some(required - 1),
    };
    assert!(matches!(
        capped
            .allocation_policy()
            .resolve::<std::convert::Infallible>(required),
        Err(ConfigurationError::Budget { .. })
    ));
}

fn profiles() -> [FrameProfile; 5] {
    [
        FrameProfile::Legacy,
        FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        FrameProfile::Hdr {
            samples: 4,
            tone_map: crate::ToneMap::Reinhard,
        },
        FrameProfile::HdrUpscaled {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
            scale: crate::RenderScale::new(500_000).unwrap(),
            upscale: crate::UpscaleSettings {
                mode: crate::UpscaleMode::Easu,
                sharpness: None,
            },
        },
        FrameProfile::HdrNativeRcas {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
            rcas: crate::NativeRcasSettings {
                stops: 0.5,
                domain: crate::RcasDomain::EncodedOutput,
            },
        },
    ]
}
